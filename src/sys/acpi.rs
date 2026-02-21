//! ACPI — Power management (shutdown/reboot)
//!
//! Minimal implementation: only supports power off via ACPI PM1a.

use x86_64::instructions::port::Port;

static mut PM1A_CNT: u32 = 0;
static mut SLP_TYPA: u16 = 0;
#[allow(dead_code)]
const  SLP_EN:       u16 = 1 << 13;

pub fn init() {
    // On QEMU, power off can be done via port 0x604
    // For real hardware, ACPI table parsing is required
    // (can be extended using the `acpi` crate)
    klog!("ACPI: init (minimal mode)");

    // QEMU power off magic
    unsafe { PM1A_CNT = 0x604; SLP_TYPA = 0; }
}

/// Shut down the system
pub fn power_off() -> ! {
    klog!("ACPI: power off...");
    unsafe {
        // QEMU: write to port 0x604
        let mut port: Port<u16> = Port::new(0x604);
        port.write(0x2000);

        // Fallback: halt loop
        loop { x86_64::instructions::hlt(); }
    }
}
