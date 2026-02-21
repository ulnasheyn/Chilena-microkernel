//! Chilena Shell — interactive command interpreter
//!
//! Supports basic commands: help, clear, echo, cd, ls, cat,
//! exit, reboot, halt, info, install, send, recv.

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

/// Run the interactive shell
pub fn run_interactive() -> Result<(), ExitCode> {
    println!("{}", BANNER);
    println!("Chilena v{} — type 'help' for commands.\n", crate::VERSION);

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

/// Run a shell script from a file
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
        "help"    => cmd_help(),
        "clear"   => cmd_clear(),
        "echo"    => cmd_echo(args),
        "cd"      => cmd_cd(args),
        "ls"      => cmd_ls(args),
        "cat"     => cmd_cat(args),
        "write"   => cmd_write(args),
        "info"    => crate::usr::info::run(),
        "reboot"  => cmd_reboot(),
        "halt"    => cmd_halt(),
        "exit"    => return Err(ExitCode::Success),
        "install" => cmd_install(),
        "send"    => cmd_send(args),
        "recv"    => cmd_recv(),
        other     => {
            println!("Unknown command: '{}'. Type 'help' for a list of commands.", other);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Command implementations
// ---------------------------------------------------------------------------

fn cmd_help() {
    println!("Available commands:");
    println!("  help           — show this message");
    println!("  clear          — clear the screen");
    println!("  echo [text]    — print text");
    println!("  cd [path]      — change directory");
    println!("  ls             — list files (in-memory)");
    println!("  cat [file]     — show file contents");
    println!("  write [f] [t]  — write text to file");
    println!("  info           — system information");
    println!("  reboot         — restart the system");
    println!("  halt           — shut down the system");
    println!("  install        — setup initial filesystem");
    println!("  send <pid> <m> — send message to process");
    println!("  recv           — wait and display incoming message");
    println!("  exit           — exit the shell");
}

fn cmd_clear() {
    print!("\x1b[2J\x1b[H"); // ANSI: clear screen + cursor to home
}

fn cmd_echo(args: &[&str]) {
    println!("{}", args.join(" "));
}

fn cmd_cd(args: &[&str]) {
    let path = args.first().copied().unwrap_or("/");
    sys::process::set_cwd(path);
}

fn cmd_ls(_args: &[&str]) {
    // Show files in VFS (in-memory)
    println!("(in-memory filesystem — use 'write' to create files, 'cat' to read)");
    println!("  /ini/boot.sh");
}

fn cmd_cat(args: &[&str]) {
    let path = match args.first() {
        Some(p) => p,
        None => { println!("cat: filename required"); return; }
    };

    let full_path = match sys::fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => { println!("cat: invalid path"); return; }
    };

    if let Some(mut f) = sys::fs::open_file(&full_path) {
        let mut buf = alloc::vec![0u8; f.size().max(1)];
        if let Ok(n) = f.read(&mut buf) {
            let s = String::from_utf8_lossy(&buf[..n]);
            print!("{}", s);
            if !s.ends_with('\n') { println!(); }
        }
    } else {
        println!("cat: file '{}' not found", path);
    }
}

fn cmd_write(args: &[&str]) {
    if args.len() < 2 {
        println!("write: usage: write <file> <text>");
        return;
    }
    let path = args[0];
    let text = args[1..].join(" ");
    let full_path = match sys::fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => { println!("write: invalid path"); return; }
    };
    let mut data = text.as_bytes().to_vec();
    data.push(b'\n');
    if sys::fs::write_file(&full_path, &data).is_ok() {
        println!("Written to '{}'", full_path);
    } else {
        println!("write: failed to write to '{}'", full_path);
    }
}

fn cmd_send(args: &[&str]) {
    if args.len() < 2 {
        println!("send: usage: send <pid> <message>");
        println!("example: send 1 hello");
        return;
    }
    let pid: usize = match args[0].parse() {
        Ok(p) => p,
        Err(_) => { println!("send: pid must be a number"); return; }
    };
    let message = args[1..].join(" ");
    let result = crate::api::syscall::send(pid, 0, message.as_bytes());
    if result == usize::MAX {
        println!("send: failed to send to PID {}", pid);
    } else {
        println!("send: message sent to PID {}", pid);
    }
}

fn cmd_recv() {
    println!("recv: waiting for message...");
    let mut msg = crate::sys::ipc::Message::empty();
    let result = crate::api::syscall::recv(&mut msg);
    if result == 0 {
        let data = &msg.data[..msg.data.iter().position(|&b| b == 0).unwrap_or(64)];
        let text = alloc::string::String::from_utf8_lossy(data);
        println!("recv: message from PID {} > {}", msg.sender, text);
    } else {
        println!("recv: failed to receive message");
    }
}

fn cmd_install() {
    if sys::fs::is_mounted() {
        println!("Chilena is already installed!");
        return;
    }
    println!("Installing Chilena...");
    sys::fs::mount_memfs();
    sys::fs::write_file("/ini/boot.sh", b"shell\n").ok();
    sys::fs::write_file("/ini/readme.txt", b"Welcome to Chilena!\n").ok();
    println!("Installation complete! Type 'reboot' to restart.");
}

fn cmd_reboot() {
    println!("Rebooting...");
    unsafe { crate::sys::syscall::syscall1(crate::sys::syscall::number::HALT, 0xCAFE); }
}

fn cmd_halt() {
    println!("Shutting down...");
    unsafe { crate::sys::syscall::syscall1(crate::sys::syscall::number::HALT, 0xDEAD); }
}
