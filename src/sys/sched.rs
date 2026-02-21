//! Scheduler for Chilena — Round-Robin Preemptive (Proper Context Switch)
//!
//! How it works:
//!   - IRQ 0 (timer) calls timer_handler via naked function
//!   - All registers are saved to the stack then to the Process struct
//!   - Round-robin selects the next Running process
//!   - Registers of the target process are restored via iretq

use crate::sys::process::{
    CURRENT_PID, NEXT_PID, PROC_TABLE,
    save_registers, save_stack_frame,
    CpuRegisters,
};
use crate::sys::ipc::BlockState;
use crate::sys::gdt::GDT;

use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::registers::control::Cr3;

// ---------------------------------------------------------------------------
// Scheduler interval
// ---------------------------------------------------------------------------

/// Switch process every 10ms (10 ticks @ 1000Hz)
const SCHED_INTERVAL: u64 = 10;

static TICK: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// tick() — called from clk::on_tick every timer interrupt
// ---------------------------------------------------------------------------

pub fn tick() {
    let t = TICK.fetch_add(1, Ordering::Relaxed);
    if t % SCHED_INTERVAL != 0 {
        return;
    }

    let n_procs = NEXT_PID.load(Ordering::SeqCst);
    if n_procs <= 1 {
        return;
    }

    let cur = CURRENT_PID.load(Ordering::SeqCst);

    let next = {
        let table = PROC_TABLE.read();
        let mut found = None;
        for i in 1..n_procs {
            let candidate = (cur + i) % n_procs;
            if candidate == 0 { continue; }
            if table[candidate].block == BlockState::Running {
                found = Some(candidate);
                break;
            }
        }
        found
    };

    if let Some(next_pid) = next {
        if next_pid != cur {
            switch_to(next_pid);
        }
    }
}

// ---------------------------------------------------------------------------
// Proper context switch — save old process state, restore new process
// ---------------------------------------------------------------------------

/// Called from timer_handler with already-saved frame and regs
pub fn schedule(
    frame: &mut x86_64::structures::idt::InterruptStackFrame,
    regs:  &mut CpuRegisters,
) {
    let t = TICK.fetch_add(1, Ordering::Relaxed);
    if t % SCHED_INTERVAL != 0 {
        return;
    }

    let n_procs = NEXT_PID.load(Ordering::SeqCst);
    if n_procs <= 1 {
        return;
    }

    let cur = CURRENT_PID.load(Ordering::SeqCst);

    // Save current process state
    save_stack_frame(**frame);
    save_registers(*regs);

    // Find next ready process
    let next = {
        let table = PROC_TABLE.read();
        let mut found = None;
        for i in 1..n_procs {
            let candidate = (cur + i) % n_procs;
            if candidate == 0 { continue; }
            if table[candidate].block == BlockState::Running {
                found = Some(candidate);
                break;
            }
        }
        found
    };

    if let Some(next_pid) = next {
        if next_pid == cur { return; }

        // Load target process state
        let (next_frame, next_regs, pt_frame) = {
            let table = PROC_TABLE.read();
            let p = &table[next_pid];
            (p.stack_frame, p.saved_regs, p.pt_frame)
        };

        CURRENT_PID.store(next_pid, Ordering::SeqCst);

        // Restore target process registers
        *regs = next_regs;

        // Restore stack frame (RIP, RSP, RFLAGS, CS, SS)
        if let Some(sf) = next_frame {
            unsafe { frame.as_mut().write(sf); }
        }

        // Switch page table
        unsafe {
            let (_, flags) = Cr3::read();
            Cr3::write(pt_frame, flags);
        }
    }
}

// ---------------------------------------------------------------------------
// Fallback switch_to — used when no saved frame exists yet
// ---------------------------------------------------------------------------

fn switch_to(next_pid: usize) {
    let (entry, stack, pt_frame, saved_regs) = {
        let table = PROC_TABLE.read();
        let p = &table[next_pid];
        (
            p.code_base + p.entry_point,
            p.stack_base,
            p.pt_frame,
            p.saved_regs,
        )
    };

    CURRENT_PID.store(next_pid, Ordering::SeqCst);

    unsafe {
        let (_, flags) = Cr3::read();
        Cr3::write(pt_frame, flags);

        core::arch::asm!(
            "cli",
            "push {ss:r}",
            "push {rsp:r}",
            "push 0x200",
            "push {cs:r}",
            "push {rip:r}",
            "iretq",
            ss  = in(reg) GDT.1.u_data.0,
            rsp = in(reg) stack,
            cs  = in(reg) GDT.1.u_code.0,
            rip = in(reg) entry,
            in("rax") saved_regs.rax,
            in("rdi") saved_regs.rdi,
            in("rsi") saved_regs.rsi,
            in("rdx") saved_regs.rdx,
            options(noreturn)
        );
    }
}
