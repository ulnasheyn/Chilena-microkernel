//! Shell Chilena — command interpreter interaktif
//!
//! Mendukung perintah dasar: help, clear, echo, cd, ls, cat,
//! exit, reboot, halt, dan info sistem.

use crate::sys;
use crate::api::process::ExitCode;
use crate::sys::fs::FileIO;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

const PROMPT: &str = "\x1b[36mchilena\x1b[0m:\x1b[33m{cwd}\x1b[0m$ ";
const BANNER: &str = r"
  ____  _     _ _                
 / ___|| |__ (_) | ___ _ __  __ _
| |    | '_ \| | |/ _ \ '_ \/ _` |
| |___ | | | | | |  __/ | | | (_| |
 \____||_| |_|_|_|\___|_| |_|\__,_|
                                    
";

/// Jalankan shell interaktif
pub fn run_interactive() -> Result<(), ExitCode> {
    println!("{}", BANNER);
    println!("Chilena v{} — ketik 'help' untuk bantuan.\n", crate::VERSION);

    loop {
        let prompt = build_prompt();
        print!("{}", prompt);

        let line = sys::console::read_line();
        let line = line.trim().to_string();

        if line.is_empty() { continue; }

        if let Err(ExitCode::Success) = exec_line(&line) {
            break;
        }
    }
    Ok(())
}

/// Jalankan skrip shell dari file
pub fn run_script(path: &str) -> Result<(), ExitCode> {
    if let Some(mut f) = sys::fs::open_file(path) {
        let mut buf = alloc::vec![0u8; f.size()];
        if let Ok(n) = f.read(&mut buf) {
            let content = String::from_utf8_lossy(&buf[..n]);
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') { continue; }
                exec_line(line).ok();
            }
            Ok(())
        } else {
            Err(ExitCode::IoError)
        }
    } else {
        Err(ExitCode::NotFound)
    }
}

fn build_prompt() -> String {
    let cwd = sys::process::cwd();
    PROMPT.replace("{cwd}", &cwd)
}

fn exec_line(line: &str) -> Result<(), ExitCode> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.is_empty() { return Ok(()); }

    let cmd  = parts[0];
    let args = &parts[1..];

    match cmd {
        "help"   => cmd_help(),
        "clear"  => cmd_clear(),
        "echo"   => cmd_echo(args),
        "cd"     => cmd_cd(args),
        "ls"     => cmd_ls(args),
        "cat"    => cmd_cat(args),
        "write"  => cmd_write(args),
        "info"   => crate::usr::info::run(),
        "reboot" => cmd_reboot(),
        "halt"   => cmd_halt(),
        "exit"    => return Err(ExitCode::Success),
        "install" => cmd_install(),
        other    => {
            println!("Perintah tidak dikenal: '{}'. Ketik 'help' untuk daftar perintah.", other);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Implementasi perintah
// ---------------------------------------------------------------------------

fn cmd_help() {
    println!("Perintah yang tersedia:");
    println!("  help           — tampilkan pesan ini");
    println!("  clear          — bersihkan layar");
    println!("  echo [teks]    — cetak teks");
    println!("  cd [path]      — ganti direktori");
    println!("  ls             — daftar file (in-memory)");
    println!("  cat [file]     — tampilkan isi file");
    println!("  write [f] [t]  — tulis teks ke file");
    println!("  info           — informasi sistem");
    println!("  reboot         — restart sistem");
    println!("  halt           — matikan sistem");
    println!("  install        — setup filesystem awal");
    println!("  exit           — keluar dari shell");
}

fn cmd_clear() {
    print!("\x1b[2J\x1b[H"); // ANSI: clear screen + cursor ke home
}

fn cmd_echo(args: &[&str]) {
    println!("{}", args.join(" "));
}

fn cmd_cd(args: &[&str]) {
    let path = args.first().copied().unwrap_or("/");
    sys::process::set_cwd(path);
}

fn cmd_ls(_args: &[&str]) {
    // Tampilkan file yang ada di VFS (in-memory)
    // Karena VFS adalah BTreeMap privat, kita tampilkan placeholder
    println!("(filesystem in-memory — gunakan 'write' untuk buat file, 'cat' untuk baca)");
    println!("  /ini/boot.sh");
}

fn cmd_cat(args: &[&str]) {
    let path = match args.first() {
        Some(p) => p,
        None => { println!("cat: perlu nama file"); return; }
    };

    let full_path = match sys::fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => { println!("cat: path tidak valid"); return; }
    };

    if let Some(mut f) = sys::fs::open_file(&full_path) {
        let mut buf = alloc::vec![0u8; f.size().max(1)];
        if let Ok(n) = f.read(&mut buf) {
            let s = String::from_utf8_lossy(&buf[..n]);
            print!("{}", s);
            if !s.ends_with('\n') { println!(); }
        }
    } else {
        println!("cat: file '{}' tidak ditemukan", path);
    }
}

fn cmd_write(args: &[&str]) {
    if args.len() < 2 {
        println!("write: perlu <file> <teks>");
        return;
    }
    let path = args[0];
    let text = args[1..].join(" ");
    let full_path = match sys::fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => { println!("write: path tidak valid"); return; }
    };
    let mut data = text.as_bytes().to_vec();
    data.push(b'\n');
    if sys::fs::write_file(&full_path, &data).is_ok() {
        println!("Berhasil ditulis ke '{}'", full_path);
    } else {
        println!("write: gagal menulis ke '{}'", full_path);
    }
}

fn cmd_install() {
    if sys::fs::is_mounted() {
        println!("Chilena sudah terinstall!");
        return;
    }
    println!("Menginstall Chilena...");
    sys::fs::mount_memfs();
    sys::fs::write_file("/ini/boot.sh", b"shell\n").ok();
    sys::fs::write_file("/ini/readme.txt", b"Selamat datang di Chilena!\n").ok();
    println!("Install selesai! Ketik 'reboot' untuk restart.");
}

fn cmd_reboot() {
    println!("Melakukan reboot...");
    unsafe { crate::sys::syscall::syscall1(crate::sys::syscall::number::HALT, 0xCAFE); }
}

fn cmd_halt() {
    println!("Mematikan sistem...");
    unsafe { crate::sys::syscall::syscall1(crate::sys::syscall::number::HALT, 0xDEAD); }
}
