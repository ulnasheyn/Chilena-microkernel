//! VGA Text Mode Driver — 80×25, 16 colors
//!
//! Writes directly to the VGA framebuffer at 0xB8000.

use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::instructions::interrupts;
use x86_64::instructions::port::Port;

// ---------------------------------------------------------------------------
// VGA constants
// ---------------------------------------------------------------------------

const VGA_ADDR: usize = 0xB8000;
const COLS: usize     = 80;
const ROWS: usize     = 25;

#[allow(dead_code)]
#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum Color {
    Black        = 0,
    Blue         = 1,
    Green        = 2,
    Cyan         = 3,
    Red          = 4,
    Magenta      = 5,
    Brown        = 6,
    LightGray    = 7,
    DarkGray     = 8,
    LightBlue    = 9,
    LightGreen   = 10,
    LightCyan    = 11,
    LightRed     = 12,
    Pink         = 13,
    Yellow       = 14,
    White        = 15,
}

#[repr(transparent)]
#[derive(Clone, Copy)]
struct Attr(u8);

impl Attr {
    const fn new(fg: Color, bg: Color) -> Self {
        Self((bg as u8) << 4 | (fg as u8))
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct VgaChar {
    ascii: u8,
    attr:  Attr,
}

// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

pub struct VgaWriter {
    col:    usize,
    row:    usize,
    attr:   Attr,
    buf:    &'static mut [[VgaChar; COLS]; ROWS],
}

impl VgaWriter {
    fn new() -> Self {
        Self {
            col:  0,
            row:  0,
            attr: Attr::new(Color::LightGray, Color::Black),
            buf:  unsafe { &mut *(VGA_ADDR as *mut [[VgaChar; COLS]; ROWS]) },
        }
    }

    fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.newline(),
            b'\r' => self.col = 0,
            b'\x08' => { // Backspace
                if self.col > 0 { self.col -= 1; }
                self.put(b' ');
                if self.col > 0 { self.col -= 1; }
            }
            byte => {
                if self.col >= COLS { self.newline(); }
                self.put(byte);
                self.col += 1;
            }
        }
    }

    fn put(&mut self, byte: u8) {
        self.buf[self.row][self.col] = VgaChar { ascii: byte, attr: self.attr };
    }

    fn newline(&mut self) {
        self.col = 0;
        if self.row < ROWS - 1 {
            self.row += 1;
        } else {
            self.scroll();
        }
    }

    fn scroll(&mut self) {
        for r in 1..ROWS {
            for c in 0..COLS {
                self.buf[r - 1][c] = self.buf[r][c];
            }
        }
        let blank = VgaChar { ascii: b' ', attr: self.attr };
        for c in 0..COLS {
            self.buf[ROWS - 1][c] = blank;
        }
    }

    fn clear(&mut self) {
        let blank = VgaChar { ascii: b' ', attr: self.attr };
        for row in self.buf.iter_mut() {
            for cell in row.iter_mut() {
                *cell = blank;
            }
        }
        self.col = 0;
        self.row = 0;
    }

    fn set_cursor(&self, row: usize, col: usize) {
        let pos = (row * COLS + col) as u16;
        unsafe {
            let mut idx: Port<u8> = Port::new(0x3D4);
            let mut val: Port<u8> = Port::new(0x3D5);
            idx.write(0x0F);
            val.write((pos & 0xFF) as u8);
            idx.write(0x0E);
            val.write((pos >> 8) as u8);
        }
    }

    /// Process minimal ANSI escape sequences (color, clear)
    fn write_str_ansi(&mut self, s: &str) {
        // Simple implementation: pass through as-is without ANSI parsing
        // (ANSI parsing is optional and can be added later)
        for byte in s.bytes() {
            self.write_byte(byte);
        }
        self.set_cursor(self.row, self.col);
    }
}

impl fmt::Write for VgaWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_str_ansi(s);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Global singleton
// ---------------------------------------------------------------------------

lazy_static! {
    pub static ref WRITER: Mutex<VgaWriter> = Mutex::new(VgaWriter::new());
}

pub fn init() {
    interrupts::without_interrupts(|| {
        WRITER.lock().clear();
    });
}
