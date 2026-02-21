//! Serial Port — UART 16550 (COM1 = 0x3F8)
//!
//! Used for early boot logging and debugging output.

use crate::sys;
use core::fmt;
use core::fmt::Write;
use lazy_static::lazy_static;
use spin::Mutex;
use uart_16550::SerialPort;
use x86_64::instructions::interrupts;

lazy_static! {
    pub static ref PORT: Mutex<SerialPort> = {
        let mut port = unsafe { SerialPort::new(0x3F8) };
        port.init();
        Mutex::new(port)
    };
}

pub fn init() {
    // Trigger lazy_static initialization
    let _ = PORT.lock();
    // IRQ 4 = COM1
    sys::idt::set_irq_handler(4, on_interrupt);
}

/// Write a string to the serial port
pub fn write_str(s: &str) {
    interrupts::without_interrupts(|| {
        PORT.lock().write_str(s).ok();
    });
}

pub fn print_fmt(args: fmt::Arguments) {
    interrupts::without_interrupts(|| {
        PORT.lock().write_fmt(args).ok();
    });
}

fn on_interrupt() {
    let byte = interrupts::without_interrupts(|| {
        PORT.lock().receive()
    });

    if byte == 0xFF { return; } // ignore invalid byte

    let ch = match byte as char {
        '\r' => '\n',
        '\x7F' => '\x08', // DEL → BS
        c => c,
    };
    sys::console::input_char(ch);
}
