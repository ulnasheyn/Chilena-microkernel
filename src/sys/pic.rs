//! PIC — Programmable Interrupt Controller (Intel 8259)
//!
//! Manages two chained PICs (master + slave) to handle
//! 16 external hardware IRQs.

use pic8259::ChainedPics;
use spin::Mutex;

/// IRQ offset in IDT (IRQ 0-7 → vectors 32-39, IRQ 8-15 → vectors 40-47)
pub const PIC_MASTER_OFFSET: u8 = 32;
pub const PIC_SLAVE_OFFSET:  u8 = PIC_MASTER_OFFSET + 8;

/// Global PIC instance
pub static PICS: Mutex<ChainedPics> = Mutex::new(unsafe {
    ChainedPics::new(PIC_MASTER_OFFSET, PIC_SLAVE_OFFSET)
});

/// Initialize PIC and enable CPU interrupts
pub fn init() {
    unsafe {
        PICS.lock().initialize();
    }
    x86_64::instructions::interrupts::enable();
}

/// Convert IRQ number to IDT vector
pub fn irq_vector(irq: u8) -> u8 {
    PIC_MASTER_OFFSET + irq
}
