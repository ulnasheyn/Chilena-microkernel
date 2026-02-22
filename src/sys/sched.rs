//! Scheduler for Chilena — Round-Robin Preemptive (Proper Context Switch)

use crate::sys::process::{
    CURRENT_PID, NEXT_PID, PROC_TABLE,
    save_registers, save_stack_frame,
    CpuRegisters, MAX_PROCS,
};
use crate::sys::ipc::BlockState;
use crate::sys::gdt::GDT;

use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::registers::control::Cr3;
use x86_64::structures::idt::{InterruptStackFrame, InterruptStackFrameValue};
use x86_64::VirtAddr;

// ---------------------------------------------------------------------------
// Scheduler interval
// ---------------------------------------------------------------------------

/// Switch process every 10ms (10 ticks @ 1000 Hz)
const SCHED_INTERVAL: u64 = 10;

static TICK: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// tick() — dipanggil dari clk::on_tick, HANYA increment counter
// Scheduling sesungguhnya ada di schedule() karena butuh akses ke stack frame
// ---------------------------------------------------------------------------

pub fn tick() {
    TICK.fetch_add(1, Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// schedule() — dipanggil dari timer_handler di idt.rs
//              dengan frame & regs yang sudah di-save oleh naked function
// ---------------------------------------------------------------------------

pub fn schedule(
    frame: &mut InterruptStackFrame,
    regs:  &mut CpuRegisters,
) {
    let t = TICK.load(Ordering::Relaxed);
    if t % SCHED_INTERVAL != 0 {
        return;
    }

    // Hitung jumlah proses aktif dengan scan tabel langsung
    // (tidak pakai NEXT_PID karena bisa tidak sinkron setelah terminate)
    let has_other = {
        let table = PROC_TABLE.read();
        let cur   = CURRENT_PID.load(Ordering::SeqCst);
        (1..MAX_PROCS).any(|i| i != cur && table[i].id != 0 && table[i].block == BlockState::Running)
    };
    if !has_other {
        return; // tidak ada proses lain yang siap jalan
    }

    let cur = CURRENT_PID.load(Ordering::SeqCst);

    // Simpan state proses yang sedang jalan
    save_stack_frame(**frame);
    save_registers(*regs);

    // Cari proses berikutnya yang ready — scan 1..MAX_PROCS (bukan 1..NEXT_PID)
    // Ini fix BUG #3: NEXT_PID tidak mencerminkan slot tertinggi yang aktif
    let next = {
        let table = PROC_TABLE.read();
        let mut found = None;
        for i in 1..MAX_PROCS {
            let candidate = if cur == 0 {
                i
            } else {
                ((cur - 1 + i) % (MAX_PROCS - 1)) + 1  // round-robin di range 1..MAX_PROCS
            };
            if candidate == 0 { continue; }
            if table[candidate].id != 0 && table[candidate].block == BlockState::Running {
                found = Some(candidate);
                break;
            }
        }
        found
    };

    let next_pid = match next {
        Some(p) if p != cur => p,
        _ => return,
    };

    // Ambil state proses berikutnya
    let (maybe_frame, next_regs, pt_frame, entry, stack) = {
        let table = PROC_TABLE.read();
        let p = &table[next_pid];
        (
            p.stack_frame,
            p.saved_regs,
            p.pt_frame,
            p.code_base + p.entry_point,
            p.stack_base,
        )
    };

    CURRENT_PID.store(next_pid, Ordering::SeqCst);

    // Restore register proses berikutnya
    *regs = next_regs;

    // Switch page table
    unsafe {
        let (_, flags) = Cr3::read();
        Cr3::write(pt_frame, flags);
    }

    // FIX BUG #2: schedule() TIDAK PERNAH iretq langsung.
    // Selalu write ke frame dan return normal ke caller (timer_handler → irq0 → iretq).
    // Ini menjaga stack kernel tetap bersih.
    unsafe {
        if let Some(sf) = maybe_frame {
            // Proses sudah pernah jalan — restore saved frame
            frame.as_mut().write(sf);
        } else {
            // Proses baru: construct frame dengan entry point dan stack userspace.
            // Setelah return dari schedule() → timer_handler → irq0 pop registers → iretq
            // CPU akan loncat ke entry point proses baru di ring 3.
            frame.as_mut().write(InterruptStackFrameValue {
                instruction_pointer: VirtAddr::new(entry),
                code_segment:        GDT.1.u_code.0 as u64,
                cpu_flags:           0x200,   // IF=1 (interrupts enabled)
                stack_pointer:       VirtAddr::new(stack),
                stack_segment:       GDT.1.u_data.0 as u64,
            });
        }
    }
}
