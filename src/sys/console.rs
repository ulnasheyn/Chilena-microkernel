//! Console — stdin buffer and kernel output
//!
//! Provides a Console device implementing FileIO,
//! so process stdout/stdin can be redirected here.

use crate::sys;
use crate::sys::fs::{FileIO, PollEvent};

use alloc::string::{String, ToString};
use core::fmt;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;
use x86_64::instructions::interrupts;

// ---------------------------------------------------------------------------
// Global console state
// ---------------------------------------------------------------------------

pub static STDIN:  Mutex<String> = Mutex::new(String::new());
pub static ECHO:   AtomicBool    = AtomicBool::new(true);
pub static RAW:    AtomicBool    = AtomicBool::new(false);

// Control characters
pub const BS:  char = '\x08'; // Backspace
pub const EOT: char = '\x04'; // End of Transmission (Ctrl+D)
pub const ESC: char = '\x1B'; // Escape
pub const ETX: char = '\x03'; // End of Text (Ctrl+C)

// ---------------------------------------------------------------------------
// Console device — implements FileIO
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Console;

impl Console {
    pub fn new() -> Self { Self }
}

impl FileIO for Console {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
        let text = if buf.len() == 4 {
            read_char().to_string()
        } else {
            read_line()
        };

        let n = text.len().min(buf.len());
        buf[..n].copy_from_slice(&text.as_bytes()[..n]);
        Ok(n)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, ()> {
        let s = String::from_utf8_lossy(buf);
        print_raw(&s);
        Ok(buf.len())
    }

    fn close(&mut self) {}

    fn poll(&mut self, event: PollEvent) -> bool {
        match event {
            PollEvent::Read  => STDIN.lock().contains('\n'),
            PollEvent::Write => true,
        }
    }

    fn kind(&self) -> u8 { 1 } // 1 = console/device
}

// ---------------------------------------------------------------------------
// Output functions
// ---------------------------------------------------------------------------

/// Print to both VGA and serial at the same time
pub fn print_fmt(args: fmt::Arguments) {
    interrupts::without_interrupts(|| {
        use fmt::Write;
        sys::vga::WRITER.lock().write_fmt(args).ok();
        sys::serial::print_fmt(args);
    });
}

fn print_raw(s: &str) {
    interrupts::without_interrupts(|| {
        use fmt::Write;
        sys::vga::WRITER.lock().write_str(s).ok();
        sys::serial::write_str(s);
    });
}

// ---------------------------------------------------------------------------
// Keyboard / serial input
// ---------------------------------------------------------------------------

/// Receive a single character from keyboard or serial
pub fn input_char(c: char) {
    let mut stdin = STDIN.lock();

    match c {
        BS => {
            if !stdin.is_empty() && ECHO.load(Ordering::SeqCst) {
                stdin.pop();
                print_raw("\x08 \x08"); // erase character on screen
            }
        }
        ETX => {
            // Ctrl+C — clear buffer and send signal
            stdin.clear();
            if ECHO.load(Ordering::SeqCst) {
                print_raw("^C\n");
            }
            stdin.push('\n');
        }
        c => {
            stdin.push(c);
            if ECHO.load(Ordering::SeqCst) && !RAW.load(Ordering::SeqCst) {
                let s = c.to_string();
                print_raw(&s);
            }
        }
    }
}

/// Read a single character from stdin (blocking)
pub fn read_char() -> char {
    loop {
        x86_64::instructions::hlt();
        let mut stdin = STDIN.lock();
        if !stdin.is_empty() {
            let c = stdin.remove(0);
            return c;
        }
    }
}

/// Read a line from stdin (blocking, until newline)
pub fn read_line() -> String {
    loop {
        x86_64::instructions::hlt();
        let mut stdin = STDIN.lock();
        if let Some(pos) = stdin.find('\n') {
            let line: String = stdin.drain(..=pos).collect();
            return line;
        }
    }
}

pub fn enable_echo()  { ECHO.store(true,  Ordering::SeqCst); }
pub fn disable_echo() { ECHO.store(false, Ordering::SeqCst); }
pub fn enable_raw()   { RAW.store(true,   Ordering::SeqCst); }
pub fn disable_raw()  { RAW.store(false,  Ordering::SeqCst); }
