//! Syscall numbers for Chilena
//!
//! Used by the kernel dispatcher and userspace library.
//! Convention: small numbers = fundamental operations.

pub const EXIT:    usize = 0x01; // Exit current process
pub const SPAWN:   usize = 0x02; // Spawn new process from ELF
pub const READ:    usize = 0x03; // Read from handle
pub const WRITE:   usize = 0x04; // Write to handle
pub const OPEN:    usize = 0x05; // Open file/device
pub const CLOSE:   usize = 0x06; // Close handle
pub const STAT:    usize = 0x07; // File metadata
pub const DUP:     usize = 0x08; // Duplicate handle
pub const REMOVE:  usize = 0x09; // Delete file
pub const HALT:    usize = 0x0A; // Halt/reboot system
pub const SLEEP:   usize = 0x0B; // Sleep for N seconds
pub const POLL:    usize = 0x0C; // Poll handle readiness
pub const ALLOC:   usize = 0x0D; // Allocate userspace memory
pub const FREE:    usize = 0x0E; // Free userspace memory
pub const KIND:    usize = 0x0F; // Handle type (file/device/socket)
pub const SEND:    usize = 0x10; // Send IPC message to process (blocks until received)
pub const RECV:    usize = 0x11; // Wait for incoming message (blocks until available)
