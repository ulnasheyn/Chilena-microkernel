//! Keyboard — PS/2 driver via IRQ 1
//!
//! Translates scan codes to Unicode characters
//! and pushes them into the console stdin buffer.

use crate::sys;
use lazy_static::lazy_static;
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
use spin::Mutex;
use x86_64::instructions::port::Port;

lazy_static! {
    static ref KB: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> = {
        Mutex::new(Keyboard::new(
            ScancodeSet1::new(),
            layouts::Us104Key,
            HandleControl::Ignore,
        ))
    };
}

pub fn init() {
    sys::idt::set_irq_handler(1, on_interrupt);
}

fn on_interrupt() {
    let scancode: u8 = unsafe { Port::<u8>::new(0x60).read() };

    let mut kb = KB.lock();
    if let Ok(Some(event)) = kb.add_byte(scancode) {
        if let Some(key) = kb.process_keyevent(event) {
            let ch = match key {
                DecodedKey::Unicode(c) => c,
                DecodedKey::RawKey(_)  => return,
            };
            sys::console::input_char(ch);
        }
    }
}
