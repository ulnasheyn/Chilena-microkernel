//! IDT — Interrupt Descriptor Table
//!
//! Mendaftarkan handler untuk:
//!   - Exception CPU (page fault, double fault, GPF, dll)
//!   - IRQ hardware 0-15
//!   - Syscall via int 0x80 (ring 3 accessible)

use crate::sys;
use crate::sys::mem::phys_mem_offset;
use crate::sys::process::CpuRegisters;

use core::arch::{asm, naked_asm};
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::instructions::interrupts;
use x86_64::instructions::port::Port;
use x86_64::registers::control::Cr2;
use x86_64::structures::idt::{
    InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode,
};
use x86_64::structures::paging::OffsetPageTable;
use x86_64::VirtAddr;

// ---------------------------------------------------------------------------
// IRQ handler table — bisa diisi oleh driver
// ---------------------------------------------------------------------------

fn noop_handler() {}

lazy_static! {
    static ref IRQ_HANDLERS: Mutex<[fn(); 16]> = Mutex::new([noop_handler; 16]);

    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        // Exception handlers
        idt.breakpoint.set_handler_fn(on_breakpoint);
        idt.stack_segment_fault.set_handler_fn(on_stack_segment_fault);
        idt.segment_not_present.set_handler_fn(on_segment_not_present);

        unsafe {
            idt.double_fault
                .set_handler_fn(on_double_fault)
                .set_stack_index(sys::gdt::DOUBLE_FAULT_IST);
            idt.page_fault
                .set_handler_fn(on_page_fault)
                .set_stack_index(sys::gdt::PAGE_FAULT_IST);
            idt.general_protection_fault
                .set_handler_fn(on_general_protection_fault)
                .set_stack_index(sys::gdt::GPF_IST);

            // Syscall gate: int 0x80, ring 3 accessible
            let syscall_addr = VirtAddr::from_ptr(syscall_entry as *const ());
            idt[0x80]
                .set_handler_addr(syscall_addr)
                .set_privilege_level(x86_64::PrivilegeLevel::Ring3);
        }

        // IRQ 0-15
        idt[sys::pic::irq_vector(0).into()].set_handler_fn(irq0);
        idt[sys::pic::irq_vector(1).into()].set_handler_fn(irq1);
        idt[sys::pic::irq_vector(2).into()].set_handler_fn(irq2);
        idt[sys::pic::irq_vector(3).into()].set_handler_fn(irq3);
        idt[sys::pic::irq_vector(4).into()].set_handler_fn(irq4);
        idt[sys::pic::irq_vector(5).into()].set_handler_fn(irq5);
        idt[sys::pic::irq_vector(6).into()].set_handler_fn(irq6);
        idt[sys::pic::irq_vector(7).into()].set_handler_fn(irq7);
        idt[sys::pic::irq_vector(8).into()].set_handler_fn(irq8);
        idt[sys::pic::irq_vector(9).into()].set_handler_fn(irq9);
        idt[sys::pic::irq_vector(10).into()].set_handler_fn(irq10);
        idt[sys::pic::irq_vector(11).into()].set_handler_fn(irq11);
        idt[sys::pic::irq_vector(12).into()].set_handler_fn(irq12);
        idt[sys::pic::irq_vector(13).into()].set_handler_fn(irq13);
        idt[sys::pic::irq_vector(14).into()].set_handler_fn(irq14);
        idt[sys::pic::irq_vector(15).into()].set_handler_fn(irq15);

        idt
    };
}

pub fn init() {
    IDT.load();
}

// ---------------------------------------------------------------------------
// IRQ dispatch helper
// ---------------------------------------------------------------------------

macro_rules! irq_fn {
    ($name:ident, $n:expr) => {
        extern "x86-interrupt" fn $name(_: InterruptStackFrame) {
            IRQ_HANDLERS.lock()[$n]();
            unsafe {
                sys::pic::PICS
                    .lock()
                    .notify_end_of_interrupt(sys::pic::irq_vector($n));
            }
        }
    };
}

irq_fn!(irq0,  0);  irq_fn!(irq1,  1);  irq_fn!(irq2,  2);  irq_fn!(irq3,  3);
irq_fn!(irq4,  4);  irq_fn!(irq5,  5);  irq_fn!(irq6,  6);  irq_fn!(irq7,  7);
irq_fn!(irq8,  8);  irq_fn!(irq9,  9);  irq_fn!(irq10, 10); irq_fn!(irq11, 11);
irq_fn!(irq12, 12); irq_fn!(irq13, 13); irq_fn!(irq14, 14); irq_fn!(irq15, 15);

// ---------------------------------------------------------------------------
// Exception handlers
// ---------------------------------------------------------------------------

extern "x86-interrupt" fn on_breakpoint(_frame: InterruptStackFrame) {
    kdebug!("EXCEPTION: BREAKPOINT\n{:#?}", frame);
    panic!("breakpoint");
}

extern "x86-interrupt" fn on_double_fault(frame: InterruptStackFrame, code: u64) -> ! {
    panic!("DOUBLE FAULT (code={}) at\n{:#?}", code, frame);
}

extern "x86-interrupt" fn on_general_protection_fault(frame: InterruptStackFrame, code: u64) {
    panic!("GENERAL PROTECTION FAULT (code={}) at\n{:#?}", code, frame);
}

extern "x86-interrupt" fn on_stack_segment_fault(frame: InterruptStackFrame, code: u64) {
    panic!("STACK SEGMENT FAULT (code={}) at\n{:#?}", code, frame);
}

extern "x86-interrupt" fn on_segment_not_present(frame: InterruptStackFrame, code: u64) {
    panic!("SEGMENT NOT PRESENT (code={}) at\n{:#?}", code, frame);
}

extern "x86-interrupt" fn on_page_fault(
    _frame: InterruptStackFrame,
    error: PageFaultErrorCode,
) {
    let fault_addr = Cr2::read().as_u64();

    let page_table = unsafe { sys::process::page_table() };
    let mut mapper = unsafe {
        OffsetPageTable::new(page_table, VirtAddr::new(phys_mem_offset()))
    };

    // Coba alokasi halaman on-demand jika proses menulis
    if error.contains(PageFaultErrorCode::CAUSED_BY_WRITE) {
        if sys::mem::map_page(&mut mapper, fault_addr, 1).is_err() {
            kerror!("Page fault: tidak bisa alokasi halaman di {:#X}", fault_addr);
            panic!("page fault");
        }
    } else {
        kerror!("Page fault di {:#X} (flags: {:?})", fault_addr, error);
        panic!("page fault");
    }
}

// ---------------------------------------------------------------------------
// Syscall entry (naked function — simpan semua scratch register)
// ---------------------------------------------------------------------------

/// Entry point syscall: simpan register, panggil dispatcher, restore
#[unsafe(naked)]
extern "sysv64" fn syscall_entry() -> ! {
    naked_asm!(
        "cld",
        "push rax",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "mov rsi, rsp",        // arg2: pointer ke saved registers
        "mov rdi, rsp",        // arg1: pointer ke interrupt frame
        "add rdi, 9 * 8",      // interrupt frame ada di atas 9 register
        "sti",                 // boleh interrupt selama eksekusi syscall
        "call {handler}",
        "cli",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rax",
        "iretq",
        handler = sym syscall_handler,
    );
}

/// Handler syscall — dipanggil dari syscall_entry
extern "sysv64" fn syscall_handler(
    frame: &mut InterruptStackFrame,
    regs:  &mut CpuRegisters,
) {
    let number = regs.rax;
    let a1 = regs.rdi;
    let a2 = regs.rsi;
    let a3 = regs.rdx;
    let a4 = regs.r8;

    // Simpan konteks sebelum spawn proses baru
    if number == sys::syscall::number::SPAWN {
        sys::process::save_stack_frame(**frame);
        sys::process::save_registers(*regs);
    }

    let result = sys::syscall::dispatch(number, a1, a2, a3, a4);

    // Restore konteks setelah proses exit
    if number == sys::syscall::number::EXIT {
        if let Some(sf) = sys::process::saved_stack_frame() {
            unsafe { frame.as_mut().write(sf); }
        }
        *regs = sys::process::saved_registers();
    }

    regs.rax = result;
}

// ---------------------------------------------------------------------------
// IRQ management API
// ---------------------------------------------------------------------------

/// Daftarkan handler untuk IRQ tertentu
pub fn set_irq_handler(irq: u8, handler: fn()) {
    interrupts::without_interrupts(|| {
        IRQ_HANDLERS.lock()[irq as usize] = handler;
        clear_irq_mask(irq);
    });
}

/// Masking IRQ (disable)
pub fn set_irq_mask(irq: u8) {
    let mut port = irq_port(irq);
    unsafe {
        let val = port.read() | (1 << irq_line(irq));
        port.write(val);
    }
}

/// Unmask IRQ (enable)
pub fn clear_irq_mask(irq: u8) {
    let mut port = irq_port(irq);
    unsafe {
        let val = port.read() & !(1 << irq_line(irq));
        port.write(val);
    }
}

fn irq_port(irq: u8) -> Port<u8> {
    Port::new(if irq < 8 { 0x21 } else { 0xA1 })
}

fn irq_line(irq: u8) -> u8 {
    if irq < 8 { irq } else { irq - 8 }
}

/// Triple fault → reboot via IDT kosong
static EMPTY_IDT: InterruptDescriptorTable = InterruptDescriptorTable::new();

pub fn trigger_reset() -> ! {
    EMPTY_IDT.load();
    unsafe { asm!("int 0", options(noreturn)); }
}
