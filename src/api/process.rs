//! Process API for Chilena

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum ExitCode {
    Success    = 0,
    Failure    = 1,
    NotFound   = 2,
    IoError    = 3,
    ExecError  = 4,
    PageFault  = 5,
}

impl From<usize> for ExitCode {
    fn from(n: usize) -> Self {
        match n {
            0 => Self::Success,
            2 => Self::NotFound,
            3 => Self::IoError,
            4 => Self::ExecError,
            5 => Self::PageFault,
            _ => Self::Failure,
        }
    }
}

impl From<ExitCode> for usize {
    fn from(e: ExitCode) -> usize { e as usize }
}

/// Exit the current process
pub fn exit(code: ExitCode) -> ! {
    unsafe { crate::sys::syscall::syscall1(crate::sys::syscall::number::EXIT, code as usize); }
    loop { x86_64::instructions::hlt(); }
}
