//! IPC — Inter-Process Communication for Chilena
//!
//! Implements synchronous message passing:
//!   - Sender blocks until receiver reads the message
//!   - Fixed-size 64-byte payload (enough for pointer + length for larger data)
//!   - Single mailbox slot per process (simple, no heap allocation)

use crate::sys::process::{current_pid, PROC_TABLE};

// ---------------------------------------------------------------------------
// Message structure
// ---------------------------------------------------------------------------

/// Message payload size in bytes
pub const MSG_PAYLOAD: usize = 64;

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Message {
    /// Sender PID
    pub sender:  usize,
    /// Message type — freely defined by userspace
    pub kind:    u32,
    /// Fixed-size payload, can hold small data or a pointer + length
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
// Process block state
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BlockState {
    /// Process is running normally
    Running,
    /// Waiting for target mailbox to be empty (during SEND)
    WaitingSend { target: usize },
    /// Waiting for incoming message (during RECV)
    WaitingRecv,
}

// ---------------------------------------------------------------------------
// send — send a message to a target process (synchronous, blocking)
// ---------------------------------------------------------------------------

/// Send a message to `target_pid`.
/// Returns: 0 = success, usize::MAX = error (invalid PID)
pub fn send(target_pid: usize, kind: u32, data: &[u8]) -> usize {
    let sender_pid = current_pid();

    // Validate target
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

    // Spin until target mailbox is empty, then deposit message
    let mut retries = 0usize;
    loop {
        let mut table = PROC_TABLE.write();

        if table[target_pid].mailbox.is_none() {
            table[target_pid].mailbox   = Some(msg);
            table[target_pid].block     = BlockState::Running;
            table[sender_pid].block     = BlockState::Running;
            return 0;
        }

        // Timeout after 1000 retries — avoid freeze on single core
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
// recv — wait for incoming message (blocking)
// ---------------------------------------------------------------------------

/// Wait and take a message from this process's mailbox.
/// Writes message to `out`, returns: 0 = success
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
            // Mailbox empty — mark as waiting
            table[pid].block = BlockState::WaitingRecv;
        }
        x86_64::instructions::hlt();
    }
}
