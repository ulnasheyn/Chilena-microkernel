#![no_std]
#![no_main]

extern crate alloc;

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use chilena::{sys, usr, hlt_loop};
use chilena::{kerror, kwarn, print};

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    chilena::init(boot_info);
    print!("\x1b[?25h");
    loop {
        boot_sequence();
    }
}

fn boot_sequence() {
    let boot_script = "/ini/boot.sh";
    if sys::fs::exists(boot_script) {
        usr::shell::run_script(boot_script).ok();
    } else {
        if sys::fs::is_mounted() {
            kerror!("Boot file '{}' not found", boot_script);
        } else {
            kwarn!("Filesystem not mounted. Run 'install' to set up.");
        }
        usr::shell::run_interactive().ok();
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if let Some(loc) = info.location() {
        kerror!("PANIC at {}:{}:{}", loc.file(), loc.line(), loc.column());
    } else {
        kerror!("PANIC: {}", info);
    }
    hlt_loop();
}
