//! IPC — Inter-Process Communication Chilena
//!
//! Implementasi synchronous message passing:
//!   - Sender block sampai receiver membaca pesan
//!   - Fixed-size payload 64 byte (cukup untuk pointer + length kalau perlu data besar)
//!   - Satu mailbox slot per proses (simple, no heap allocation)

use crate::sys::process::{current_pid, PROC_TABLE};
use core::sync::atomic::Ordering;

// ---------------------------------------------------------------------------
// Struktur pesan
// ---------------------------------------------------------------------------

/// Ukuran payload pesan dalam byte
pub const MSG_PAYLOAD: usize = 64;

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Message {
    /// PID pengirim
    pub sender:  usize,
    /// Tipe pesan — bebas didefinisikan userspace
    pub kind:    u32,
    /// Payload fixed-size, bisa berisi data kecil atau pointer + length
    pub data:    [u8; MSG_PAYLOAD],
}

impl Message {
    pub const fn empty() -> Self {
        Self {
            sender: 0,
            kind:   0,
            data:   [0u8; MSG_PAYLOAD],
        }
    }
}

// ---------------------------------------------------------------------------
// Status blokir proses
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BlockState {
    /// Proses berjalan normal
    Running,
    /// Menunggu mailbox target kosong (sedang SEND)
    WaitingSend { target: usize },
    /// Menunggu pesan masuk (sedang RECV)
    WaitingRecv,
}

// ---------------------------------------------------------------------------
// send — kirim pesan ke proses target (synchronous, blocking)
// ---------------------------------------------------------------------------

/// Kirim pesan ke `target_pid`.
/// Return: 0 = sukses, usize::MAX = error (PID tidak valid)
pub fn send(target_pid: usize, kind: u32, data: &[u8]) -> usize {
    let sender_pid = current_pid();

    // Validasi target
    {
        let table = PROC_TABLE.read();
        if target_pid >= table.len() || table[target_pid].id == 0 && target_pid != 0 {
            return usize::MAX;
        }
    }

    let mut payload = [0u8; MSG_PAYLOAD];
    let copy_len = data.len().min(MSG_PAYLOAD);
    payload[..copy_len].copy_from_slice(&data[..copy_len]);

    let msg = Message { sender: sender_pid, kind, data: payload };

    // Spin sampai mailbox target kosong, lalu deposit pesan
    let mut retries = 0usize;
    loop {
        let mut table = PROC_TABLE.write();

        if table[target_pid].mailbox.is_none() {
            table[target_pid].mailbox   = Some(msg);
            table[target_pid].block     = BlockState::Running;
            table[sender_pid].block     = BlockState::Running;
            return 0;
        }

        // Timeout setelah 1000 retry — hindari freeze di single core
        retries += 1;
        if retries > 1000 {
            table[sender_pid].block = BlockState::Running;
            return usize::MAX;
        }

        table[sender_pid].block = BlockState::WaitingSend { target: target_pid };
        drop(table);
        x86_64::instructions::hlt();
    }
}

// ---------------------------------------------------------------------------
// recv — tunggu pesan masuk (blocking)
// ---------------------------------------------------------------------------

/// Tunggu dan ambil pesan dari mailbox proses ini.
/// Menulis pesan ke `out`, return: 0 = sukses
pub fn recv(out: &mut Message) -> usize {
    let pid = current_pid();

    loop {
        {
            let mut table = PROC_TABLE.write();
            if let Some(msg) = table[pid].mailbox.take() {
                table[pid].block = BlockState::Running;
                *out = msg;
                return 0;
            }
            // Mailbox kosong — tandai sedang menunggu
            table[pid].block = BlockState::WaitingRecv;
        }
        x86_64::instructions::hlt();
    }
}
