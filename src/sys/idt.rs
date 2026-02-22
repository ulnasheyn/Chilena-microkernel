//! IDT — Interrupt Descriptor Table
//!
//! Registers handlers for:
//!   - CPU exceptions (page fault, double fault, GPF, etc.)
//!   - Hardware IRQs 0-15
//!   - Syscalls via int 0x80 (ring 3 accessible)

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
// IRQ handler table — filled by drivers
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
// IRQ dispatch helper macro
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

// IRQ 0 (timer) — naked function untuk proper context save/restore
//
// URUTAN PUSH harus cocok dengan layout struct CpuRegisters:
//   struct CpuRegisters { r15, r14, r13, r12, rbp, rbx, r11, r10, r9, r8, rdi, rsi, rdx, rcx, rax }
//   field pertama (r15) = offset 0 = [RSP+0] setelah semua push
//   field terakhir (rax) = offset 112 = [RSP+112] setelah semua push
//
// Stack tumbuh ke bawah: push terakhir = alamat terendah = RSP saat ini.
// Jadi r15 harus di-push TERAKHIR (agar di RSP+0), rax harus di-push PERTAMA.
#[unsafe(naked)]
extern "x86-interrupt" fn irq0(_: InterruptStackFrame) {
    naked_asm!(
        "cld",
        // Push scratch registers DULU (rax di offset tinggi)
        "push rax",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        // Push callee-saved TERAKHIR (r15 di offset 0 = RSP saat ini)
        "push rbx",
        "push rbp",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "mov rsi, rsp",       // arg2: &mut CpuRegisters (RSP sekarang = &r15 = field pertama)
        "mov rdi, rsp",       // arg1: akan di-adjust ke InterruptStackFrame
        "add rdi, 15 * 8",    // frame ada di atas 15 register yang di-push (15 * 8 = 120 bytes)
        "call {handler}",
        // Pop kebalikan dari push: r15 dulu (di RSP+0), rax terakhir
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbp",
        "pop rbx",
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
        handler = sym timer_handler,
    );
}

/// Timer handler — called from irq0 naked function
/// FIX: sekarang forward frame+regs ke scheduler untuk proper context switch
extern "sysv64" fn timer_handler(
    frame: &mut InterruptStackFrame,
    regs:  &mut CpuRegisters,
) {
    // Tick clock dulu (increment counter)
    IRQ_HANDLERS.lock()[0]();

    // EOI dulu sebelum schedule agar PIC tidak blocked
    unsafe {
        sys::pic::PICS
            .lock()
            .notify_end_of_interrupt(sys::pic::irq_vector(0));
    }

    // Sekarang baru schedule — bisa modifikasi frame+regs untuk context switch
    sys::sched::schedule(frame, regs);
}

irq_fn!(irq1,  1);  irq_fn!(irq2,  2);  irq_fn!(irq3,  3);
irq_fn!(irq4,  4);  irq_fn!(irq5,  5);  irq_fn!(irq6,  6);  irq_fn!(irq7,  7);
irq_fn!(irq8,  8);  irq_fn!(irq9,  9);  irq_fn!(irq10, 10); irq_fn!(irq11, 11);
irq_fn!(irq12, 12); irq_fn!(irq13, 13); irq_fn!(irq14, 14); irq_fn!(irq15, 15);

// ---------------------------------------------------------------------------
// Exception handlers
// ---------------------------------------------------------------------------

extern "x86-interrupt" fn on_breakpoint(_frame: InterruptStackFrame) {
    kdebug!("EXCEPTION: BREAKPOINT\n{:#?}", _frame);
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

    // FIX BUG #8: Gunakan active_page_table() yang membaca dari CR3 langsung,
    // BUKAN sys::process::page_table() yang membaca PROC_TABLE[CURRENT_PID].pt_frame.
    // Ada race window di scheduler antara Cr3::write() dan CURRENT_PID.store(),
    // sehingga CURRENT_PID bisa menunjuk proses lama sementara CR3 sudah proses baru.
    // CR3 selalu benar karena CPU tidak mengubahnya saat interrupt, jadi pakai itu.
    let page_table = unsafe { sys::mem::active_page_table() };
    let mut mapper = unsafe {
        OffsetPageTable::new(page_table, VirtAddr::new(phys_mem_offset()))
    };

    // Try on-demand page allocation if process is writing
    if error.contains(PageFaultErrorCode::CAUSED_BY_WRITE) {
        if sys::mem::map_page(&mut mapper, fault_addr, 1).is_err() {
            kerror!("Page fault: could not allocate page at {:#X}", fault_addr);
            panic!("page fault");
        }
    } else {
        kerror!("Page fault at {:#X} (flags: {:?})", fault_addr, error);
        panic!("page fault");
    }
}

// ---------------------------------------------------------------------------
// Syscall entry (naked function — save all scratch registers)
// ---------------------------------------------------------------------------

/// Syscall entry point: save registers, call dispatcher, restore
//
// Urutan push harus cocok dengan CpuRegisters struct:
//   rax di-push pertama (offset 112), r15 di-push terakhir (offset 0)
#[unsafe(naked)]
extern "sysv64" fn syscall_entry() -> ! {
    naked_asm!(
        "cld",
        // Push scratch registers dulu (offset tinggi)
        "push rax",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        // Push callee-saved terakhir (r15 di RSP+0)
        "push rbx",
        "push rbp",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "mov rsi, rsp",        // arg2: &mut CpuRegisters
        "mov rdi, rsp",        // arg1: akan di-adjust ke InterruptStackFrame
        "add rdi, 15 * 8",     // frame ada di atas 15 register
        "sti",                 // allow interrupts during syscall
        "call {handler}",
        "cli",
        // Pop kebalikan push
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbp",
        "pop rbx",
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

/// Syscall handler — called from syscall_entry
extern "sysv64" fn syscall_handler(
    frame: &mut InterruptStackFrame,
    regs:  &mut CpuRegisters,
) {
    let number = regs.rax;
    let a1 = regs.rdi;
    let a2 = regs.rsi;
    let a3 = regs.rdx;
    let a4 = regs.r8;

    // Save context before spawning a new process
    if number == sys::syscall::number::SPAWN {
        sys::process::save_stack_frame(**frame);
        sys::process::save_registers(*regs);
    }

    let result = sys::syscall::dispatch(number, a1, a2, a3, a4);

    // Restore context after process exit.
    // FIX BUG #9: Setelah dispatch(EXIT) → terminate() sudah jalan,
    // CURRENT_PID sekarang = parent_id.
    // Kalau parent punya saved_stack_frame (sudah pernah di-save saat SPAWN) → restore.
    // Kalau tidak ada (parent adalah kernel/PID 0 atau belum pernah spawn) →
    // biarkan frame apa adanya, parent akan lanjut dari titik setelah syscall ini.
    if number == sys::syscall::number::EXIT {
        // saved_stack_frame() sekarang membaca dari parent (CURRENT_PID sudah berubah)
        if let Some(sf) = sys::process::saved_stack_frame() {
            unsafe { frame.as_mut().write(sf); }
            *regs = sys::process::saved_registers();
        }
        // Jika None: parent tidak punya saved frame → tidak perlu restore,
        // iretq akan kembali ke titik parent memanggil syscall SPAWN sebelumnya.
        // regs.rax akan di-set ke result di bawah (exit code).
    }

    regs.rax = result;
}

// ---------------------------------------------------------------------------
// IRQ management API
// ---------------------------------------------------------------------------

/// Register a handler for a specific IRQ
pub fn set_irq_handler(irq: u8, handler: fn()) {
    interrupts::without_interrupts(|| {
        IRQ_HANDLERS.lock()[irq as usize] = handler;
        clear_irq_mask(irq);
    });
}

/// Mask an IRQ (disable)
pub fn set_irq_mask(irq: u8) {
    let mut port = irq_port(irq);
    unsafe {
        let val = port.read() | (1 << irq_line(irq));
        port.write(val);
    }
}

/// Unmask an IRQ (enable)
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

/// Triple fault → reboot via empty IDT
static EMPTY_IDT: InterruptDescriptorTable = InterruptDescriptorTable::new();

pub fn trigger_reset() -> ! {
    EMPTY_IDT.load();
    unsafe { asm!("int 0", options(noreturn)); }
}
