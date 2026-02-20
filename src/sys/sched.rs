//! Scheduler Chilena — Round-Robin Preemptive
//!
//! Cara kerja:
//!   - Dipanggil setiap timer tick (IRQ 0) via clk::on_tick
//!   - Setiap SCHED_INTERVAL tick → cari proses berikutnya yang Running
//!   - Simpan state proses lama, restore state proses baru
//!   - Skip proses yang sedang WaitingSend / WaitingRecv

use crate::sys::process::{CURRENT_PID, NEXT_PID, PROC_TABLE};
use crate::sys::ipc::BlockState;
use crate::sys::gdt::GDT;

use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::registers::control::Cr3;

// ---------------------------------------------------------------------------
// Interval scheduler — switch setiap N tick (1 tick = ~1ms)
// ---------------------------------------------------------------------------

/// Switch proses setiap 10ms
const SCHED_INTERVAL: u64 = 10;

static TICK: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// Fungsi utama — dipanggil dari IRQ 0 handler
// ---------------------------------------------------------------------------

/// Dipanggil setiap timer tick — cek apakah waktunya switch proses
pub fn tick() {
    let t = TICK.fetch_add(1, Ordering::Relaxed);
    if t % SCHED_INTERVAL != 0 {
        return;
    }

    let n_procs = NEXT_PID.load(Ordering::SeqCst);
    if n_procs <= 1 {
        // Cuma proses 0 (kernel), tidak perlu switch
        return;
    }

    let cur = CURRENT_PID.load(Ordering::SeqCst);

    // Cari proses berikutnya yang siap jalan (round-robin)
    let next = {
        let table = PROC_TABLE.read();
        let mut found = None;
        for i in 1..n_procs {
            let candidate = (cur + i) % n_procs;
            if candidate == 0 { continue; } // skip kernel proc
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
// Context switch ke proses next_pid
// ---------------------------------------------------------------------------

fn switch_to(next_pid: usize) {
    // Ambil state proses tujuan
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
        // Switch page table ke proses tujuan
        let (_, flags) = Cr3::read();
        Cr3::write(pt_frame, flags);

        // Restore register dan lompat ke proses tujuan via iretq
        core::arch::asm!(
            "cli",
            "push {ss:r}",
            "push {rsp:r}",
            "push 0x200",     // RFLAGS: IF=1
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
