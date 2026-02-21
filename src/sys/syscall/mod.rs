//! Syscall dispatcher for Chilena
//!
//! Receives syscall number and raw arguments (usize),
//! converts them to proper types, then calls the service layer.

pub mod number;
pub mod service;

use crate::api::process::ExitCode;
use crate::sys;
use crate::sys::fs::FileInfo;

use core::arch::asm;

fn raw_str(ptr: *mut u8, len: usize) -> &'static str {
    unsafe {
        let slice = core::slice::from_raw_parts(ptr, len);
        core::str::from_utf8_unchecked(slice)
    }
}

/// Receive syscall from IDT handler and forward to service layer
pub fn dispatch(n: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> usize {
    match n {
        number::EXIT => {
            service::exit(ExitCode::from(a1)) as usize
        }

        number::SLEEP => {
            service::sleep(f64::from_bits(a1 as u64));
            0
        }

        number::SPAWN => {
            let ptr  = sys::process::resolve_addr(a1 as u64);
            let len  = a2;
            let path = raw_str(ptr, len);
            let args_ptr = a3;
            let args_len = a4;
            service::spawn(path, args_ptr, args_len) as usize
        }

        number::HALT => {
            service::halt(a1)
        }

        number::OPEN => {
            let ptr   = sys::process::resolve_addr(a1 as u64);
            let len   = a2;
            let flags = a3 as u8;
            let path  = raw_str(ptr, len);
            service::open(path, flags) as usize
        }

        number::CLOSE => {
            service::close(a1);
            0
        }

        number::READ => {
            let handle = a1;
            let ptr = sys::process::resolve_addr(a2 as u64);
            let len = a3;
            let buf = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
            service::read(handle, buf) as usize
        }

        number::WRITE => {
            let handle = a1;
            let ptr = sys::process::resolve_addr(a2 as u64);
            let len = a3;
            let buf = unsafe { core::slice::from_raw_parts(ptr, len) };
            service::write(handle, buf) as usize
        }

        number::DUP => {
            service::dup(a1, a2) as usize
        }

        number::STAT => {
            let ptr  = sys::process::resolve_addr(a1 as u64);
            let len  = a2;
            let path = raw_str(ptr, len);
            let info = unsafe { &mut *(a3 as *mut FileInfo) };
            service::stat(path, info) as usize
        }

        number::REMOVE => {
            let ptr  = sys::process::resolve_addr(a1 as u64);
            let len  = a2;
            let path = raw_str(ptr, len);
            service::remove(path) as usize
        }

        number::KIND => {
            service::kind(a1) as usize
        }

        number::SEND => {
            // a1 = target_pid, a2 = kind, a3 = data_ptr, a4 = data_len
            let target  = a1;
            let kind    = a2 as u32;
            let ptr     = sys::process::resolve_addr(a3 as u64);
            let len     = a4;
            let data    = unsafe { core::slice::from_raw_parts(ptr, len) };
            sys::ipc::send(target, kind, data)
        }

        number::RECV => {
            // a1 = pointer to Message struct in userspace
            let out = unsafe { &mut *(sys::process::resolve_addr(a1 as u64) as *mut sys::ipc::Message) };
            sys::ipc::recv(out)
        }

        number::POLL => {
            let ptr  = sys::process::resolve_addr(a1 as u64) as *const _;
            let len  = a2;
            let list = unsafe { core::slice::from_raw_parts(ptr, len) };
            service::poll(list) as usize
        }

        number::ALLOC => {
            service::alloc_user(a1, a2) as usize
        }

        number::FREE => {
            unsafe { service::free_user(a1 as *mut u8, a2, a3) };
            0
        }

        _ => {
            kdebug!("unknown syscall: {:#X}", n);
            usize::MAX
        }
    }
}

// ---------------------------------------------------------------------------
// Syscall helper functions for userspace (used from api/syscall.rs)
// ---------------------------------------------------------------------------

pub unsafe fn syscall0(n: usize) -> usize {
    let r: usize;
    asm!("int 0x80", in("rax") n, lateout("rax") r);
    r
}

pub unsafe fn syscall1(n: usize, a1: usize) -> usize {
    let r: usize;
    asm!("int 0x80", in("rax") n, in("rdi") a1, lateout("rax") r);
    r
}

pub unsafe fn syscall2(n: usize, a1: usize, a2: usize) -> usize {
    let r: usize;
    asm!("int 0x80", in("rax") n, in("rdi") a1, in("rsi") a2, lateout("rax") r);
    r
}

pub unsafe fn syscall3(n: usize, a1: usize, a2: usize, a3: usize) -> usize {
    let r: usize;
    asm!(
        "int 0x80",
        in("rax") n, in("rdi") a1, in("rsi") a2, in("rdx") a3,
        lateout("rax") r
    );
    r
}

pub unsafe fn syscall4(n: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> usize {
    let r: usize;
    asm!(
        "int 0x80",
        in("rax") n, in("rdi") a1, in("rsi") a2, in("rdx") a3, in("r8") a4,
        lateout("rax") r
    );
    r
}

/// Macro shorthand for syscalls
#[macro_export]
macro_rules! syscall {
    ($n:expr)                         => { $crate::sys::syscall::syscall0($n as usize) };
    ($n:expr, $a1:expr)               => { $crate::sys::syscall::syscall1($n as usize, $a1 as usize) };
    ($n:expr, $a1:expr, $a2:expr)     => { $crate::sys::syscall::syscall2($n as usize, $a1 as usize, $a2 as usize) };
    ($n:expr, $a1:expr, $a2:expr, $a3:expr) => {
        $crate::sys::syscall::syscall3($n as usize, $a1 as usize, $a2 as usize, $a3 as usize)
    };
    ($n:expr, $a1:expr, $a2:expr, $a3:expr, $a4:expr) => {
        $crate::sys::syscall::syscall4($n as usize, $a1 as usize, $a2 as usize, $a3 as usize, $a4 as usize)
    };
}
