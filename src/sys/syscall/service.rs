//! Implementasi layanan syscall Chilena
//!
//! Setiap fungsi di sini adalah backend dari satu syscall.
//! Dipanggil oleh dispatcher di mod.rs.

use crate::api::process::ExitCode;
use crate::sys;

use crate::sys::process::Process;

use alloc::vec;
use core::alloc::Layout;

// ---------------------------------------------------------------------------
// Proses
// ---------------------------------------------------------------------------

pub fn exit(code: ExitCode) -> ExitCode {
    sys::process::terminate();
    code
}

pub fn sleep(seconds: f64) {
    sys::clk::sleep(seconds);
}

pub fn spawn(path: &str, args_ptr: usize, args_len: usize) -> ExitCode {
    let path = match sys::fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => return ExitCode::NotFound,
    };

    if let Some(mut file) = sys::fs::open_file(&path) {
        use crate::sys::fs::FileIO;
        let mut buf = vec![0u8; file.size()];
        if let Ok(n) = file.read(&mut buf) {
            buf.truncate(n);
            match Process::spawn(&buf, args_ptr, args_len) {
                Ok(_) => unreachable!(), // kernel berpindah ke proses anak
                Err(e) => e,
            }
        } else {
            ExitCode::IoError
        }
    } else {
        ExitCode::NotFound
    }
}

pub fn halt(code: usize) -> usize {
    match code {
        0xCAFE => sys::idt::trigger_reset(),
        0xDEAD => {
            sys::process::terminate();
            sys::acpi::power_off();
        }
        _ => kdebug!("HALT: kode tidak dikenal {:#X}", code),
    }
    0
}

// ---------------------------------------------------------------------------
// File / handle
// ---------------------------------------------------------------------------

pub fn open(path: &str, flags: u8) -> isize {
    let path = match sys::fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => return -1,
    };
    if let Some(res) = sys::fs::open_resource(&path, flags) {
        if let Ok(h) = sys::process::alloc_handle(res) {
            return h as isize;
        }
    }
    -1
}

pub fn close(handle: usize) {
    if let Some(mut res) = sys::process::get_handle(handle) {
        res.close();
        sys::process::free_handle(handle);
    }
}

pub fn read(handle: usize, buf: &mut [u8]) -> isize {
    if let Some(mut res) = sys::process::get_handle(handle) {
        if let Ok(n) = res.read(buf) {
            sys::process::update_handle(handle, *res);
            return n as isize;
        }
    }
    -1
}

pub fn write(handle: usize, buf: &[u8]) -> isize {
    if let Some(mut res) = sys::process::get_handle(handle) {
        if let Ok(n) = res.write(buf) {
            sys::process::update_handle(handle, *res);
            return n as isize;
        }
    }
    -1
}

pub fn dup(src: usize, dst: usize) -> isize {
    if let Some(res) = sys::process::get_handle(src) {
        sys::process::update_handle(dst, *res);
        return 0;
    }
    -1
}

pub fn stat(path: &str, info: &mut sys::fs::FileInfo) -> isize {
    let path = match sys::fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => return -1,
    };
    if let Some(i) = sys::fs::stat(&path) {
        *info = i;
        0
    } else {
        -1
    }
}

pub fn remove(path: &str) -> isize {
    if sys::fs::remove(path).is_ok() { 0 } else { -1 }
}

pub fn kind(handle: usize) -> isize {
    if let Some(res) = sys::process::get_handle(handle) {
        res.kind() as isize
    } else {
        -1
    }
}

pub fn poll(handles: &[(usize, sys::fs::PollEvent)]) -> isize {
    for (i, (handle, event)) in handles.iter().enumerate() {
        if let Some(mut res) = sys::process::get_handle(*handle) {
            if res.poll(*event) {
                return i as isize;
            }
        }
    }
    -1
}

// ---------------------------------------------------------------------------
// Memori userspace
// ---------------------------------------------------------------------------

pub fn alloc_user(size: usize, align: usize) -> *mut u8 {
    Layout::from_size_align(size, align)
        .map(|layout| unsafe { sys::process::user_alloc(layout) })
        .unwrap_or(core::ptr::null_mut())
}

pub unsafe fn free_user(ptr: *mut u8, size: usize, align: usize) {
    if let Ok(layout) = Layout::from_size_align(size, align) {
        sys::process::user_free(ptr, layout);
    }
}
