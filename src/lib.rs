#![no_std]
#![cfg_attr(test, no_main)]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => { $crate::sys::console::print_fmt(format_args!($($arg)*)) };
}
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}
#[macro_export]
macro_rules! klog {
    ($($arg:tt)*) => {{ if !cfg!(test) {
        let t = $crate::sys::clk::uptime_secs();
        $crate::sys::console::print_fmt(format_args!("\x1b[32m[{:8.3}]\x1b[0m {}\n", t, format_args!($($arg)*)));
    }}};
}
#[macro_export]
macro_rules! kerror {
    ($($arg:tt)*) => {{ $crate::sys::console::print_fmt(format_args!("\x1b[31mError:\x1b[0m {}\n", format_args!($($arg)*))); }};
}
#[macro_export]
macro_rules! kwarn {
    ($($arg:tt)*) => {{ $crate::sys::console::print_fmt(format_args!("\x1b[33mWarn:\x1b[0m {}\n", format_args!($($arg)*))); }};
}
#[macro_export]
macro_rules! kdebug {
    ($($arg:tt)*) => {{ #[cfg(debug_assertions)] $crate::sys::console::print_fmt(format_args!("\x1b[34mDebug:\x1b[0m {}\n", format_args!($($arg)*))); }};
}

pub mod sys;
pub mod api;
pub mod usr;

use bootloader::BootInfo;
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn init(boot_info: &'static BootInfo) {
    sys::vga::init();
    sys::gdt::init();
    sys::idt::init();
    // mem::init HARUS sebelum pic::init karena pic::init mengaktifkan interrupt (sti).
    // Setelah interrupt aktif, timer bisa fire dan scheduler akan akses PROC_TABLE
    // yang membutuhkan heap (Box::new). Jadi heap harus sudah siap dulu.
    sys::mem::init(boot_info);
    sys::pic::init();
    sys::serial::init();
    sys::keyboard::init();
    sys::clk::init();
    klog!("SYS Chilena v{}", VERSION);
    sys::cpu::init();
    sys::acpi::init();
    klog!("RTC {}", sys::clk::date_string());
}

pub fn hlt_loop() -> ! { loop { x86_64::instructions::hlt(); } }

pub trait Testable { fn run(&self); }
impl<T: Fn()> Testable for T {
    fn run(&self) {
        print!("test {} ... ", core::any::type_name::<T>());
        self();
        println!("ok");
    }
}
pub fn test_runner(tests: &[&dyn Testable]) {
    let n = tests.len();
    println!("\nrunning {} test{}", n, if n == 1 { "" } else { "s" });
    for test in tests { test.run(); }
    exit_qemu(QemuExitCode::Success);
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode { Success = 0x10, Failed = 0x11 }
pub fn exit_qemu(code: QemuExitCode) {
    use x86_64::instructions::port::Port;
    unsafe { let mut port = Port::new(0xF4); port.write(code as u32); }
}
#[allow(dead_code)]
#[alloc_error_handler]
fn on_alloc_error(layout: alloc::alloc::Layout) -> ! {
    panic!("alloc error: could not allocate {} bytes", layout.size());
}
#[cfg(test)] use bootloader::entry_point;
#[cfg(test)] use core::panic::PanicInfo;
#[cfg(test)] entry_point!(test_kernel_main);
#[cfg(test)] fn test_kernel_main(boot_info: &'static BootInfo) -> ! { init(boot_info); test_main(); hlt_loop(); }
#[cfg(test)] #[panic_handler] fn panic(info: &PanicInfo) -> ! { println!("PANIC: {}", info); exit_qemu(QemuExitCode::Failed); hlt_loop(); }
#[test_case] fn trivial_assertion() { assert_eq!(1, 1); }
