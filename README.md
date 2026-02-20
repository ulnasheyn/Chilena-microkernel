# Chilena Microkernel

> A minimal, experimental x86_64 microkernel written in Rust (`no_std`).

Chilena is a bare-metal kernel built from scratch — no libc, no OS layer, just raw hardware. Inspired by the design philosophy of [MOROS](https://github.com/vinc/moros), but written independently with a different architecture. Every module, function, and structure was written from the ground up.

> Think of MOROS as a mature kernel, and Chilena as a newborn — same foundation, different soul.

---

## Features

- **Process Management** — ELF loader, process table (max 8), ring 0/3 separation
- **IPC (Inter-Process Communication)** — synchronous message passing via `SEND`/`RECV` syscalls
- **Round-Robin Scheduler** — preemptive, hooks into IRQ 0 (PIT timer @ 1000Hz)
- **Proper Context Switch** — full register save/restore via naked IRQ handler
- **Memory Management** — bitmap frame allocator, x86_64 paging, kernel heap
- **In-Memory VFS** — lightweight virtual filesystem, persistent per session
- **16 Syscalls** — via `int 0x80`, System V ABI convention
- **Interactive Shell** — with `send`, `recv`, `install`, `write`, `cat`, and more
- **Drivers** — VGA text mode, PS/2 keyboard, UART serial, PIT timer, RTC, ACPI

---

## Architecture

```
src/
├── main.rs              ← entry point (boot → init → shell)
├── lib.rs               ← root library, global macros
├── sys/                 ← KERNEL LAYER
│   ├── gdt.rs           ← Global Descriptor Table + TSS
│   ├── idt.rs           ← Interrupt Descriptor Table + syscall gate
│   ├── pic.rs           ← Intel 8259 PIC (master + slave)
│   ├── sched.rs         ← Round-robin preemptive scheduler
│   ├── ipc.rs           ← Message passing (SEND/RECV)
│   ├── process.rs       ← Process table, ELF loader, context switch
│   ├── mem/
│   │   ├── bitmap.rs    ← Physical frame allocator
│   │   ├── paging.rs    ← x86_64 page table management
│   │   └── heap.rs      ← Kernel heap (linked_list_allocator)
│   ├── syscall/
│   │   ├── mod.rs       ← Syscall dispatcher
│   │   ├── number.rs    ← Syscall numbers
│   │   └── service.rs   ← Syscall implementations
│   ├── fs/mod.rs        ← In-memory VFS
│   ├── clk/mod.rs       ← PIT timer + RTC clock
│   ├── console.rs       ← stdin buffer + kernel output
│   ├── keyboard.rs      ← PS/2 keyboard driver (IRQ 1)
│   ├── serial.rs        ← UART 16550 (COM1)
│   ├── vga/mod.rs       ← VGA text mode 80×25
│   ├── cpu.rs           ← CPUID detection
│   └── acpi.rs          ← Power management (shutdown/reboot)
├── api/                 ← API LAYER (kernel ↔ userspace bridge)
│   ├── syscall.rs       ← Ergonomic syscall wrappers
│   ├── process.rs       ← ExitCode, exit()
│   ├── console.rs       ← ANSI color styles
│   └── io.rs            ← Read/write helpers
└── usr/                 ← USERSPACE LAYER
    ├── shell.rs         ← Interactive shell
    ├── info.rs          ← System info command
    └── help.rs          ← Help command
```

---

## Syscall Table

Called via `int 0x80` with System V ABI convention (`rdi`, `rsi`, `rdx`, `r8`, `r9`).

| Number | Name   | Description                        |
|--------|--------|------------------------------------|
| 0x01   | EXIT   | Exit current process               |
| 0x02   | SPAWN  | Spawn a new process from ELF/CHN   |
| 0x03   | READ   | Read from a handle                 |
| 0x04   | WRITE  | Write to a handle                  |
| 0x05   | OPEN   | Open a file or device              |
| 0x06   | CLOSE  | Close a handle                     |
| 0x07   | STAT   | Get file metadata                  |
| 0x08   | DUP    | Duplicate a handle                 |
| 0x09   | REMOVE | Delete a file                      |
| 0x0A   | HALT   | Halt or reboot the system          |
| 0x0B   | SLEEP  | Sleep for N seconds                |
| 0x0C   | POLL   | Poll I/O readiness                 |
| 0x0D   | ALLOC  | Allocate userspace memory          |
| 0x0E   | FREE   | Free userspace memory              |
| 0x0F   | KIND   | Query handle type                  |
| 0x10   | SEND   | Send IPC message to a process      |
| 0x11   | RECV   | Receive IPC message (blocking)     |

---

## Shell Commands

| Command           | Description                        |
|-------------------|------------------------------------|
| `help`            | Show available commands            |
| `info`            | System info (CPU, RAM, uptime)     |
| `echo [text]`     | Print text to screen               |
| `clear`           | Clear the screen                   |
| `cd [path]`       | Change working directory           |
| `ls`              | List files in VFS                  |
| `cat [file]`      | Show file contents                 |
| `write [f] [t]`   | Write text to a file               |
| `install`         | Setup initial filesystem           |
| `send <pid> <msg>`| Send IPC message to a process      |
| `recv`            | Wait and receive an IPC message    |
| `reboot`          | Restart the system                 |
| `halt`            | Shutdown the system                |
| `exit`            | Exit the shell                     |

---

## Build & Run

### Prerequisites

```bash
# 1. Install Rust + correct toolchain (version matters!)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source $HOME/.cargo/env

rustup toolchain install nightly-2025-09-01
rustup component add rust-src llvm-tools-preview --toolchain nightly-2025-09-01

# 2. Install bootimage
cargo install bootimage

# 3. Install QEMU
sudo apt update && sudo apt install -y qemu-system-x86

# 4. Install linker
sudo apt install -y lld
```

### Build

```bash
make build
```

### Run (QEMU headless)

```bash
make image && qemu-system-x86_64 \
    -drive format=raw,file=target/x86_64-chilena/release/bootimage-chilena.bin \
    -serial mon:stdio \
    -m 256M \
    --no-reboot \
    -nographic
```

> Toolchain `nightly-2025-09-01` is required. Different versions will cause build errors.

---

## IPC Demo

Once booted, try this in the Chilena shell:

```
chilena:/$ install
chilena:/$ send 0 hello
send: pesan terkirim ke PID 0
chilena:/$ recv
recv: pesan dari PID 0 > hello
```

---

## Roadmap

- [ ] Persistent storage (ATA/VirtIO disk driver)
- [ ] More syscalls (`mkdir`, `listdir`, `pipe`)
- [ ] Memory protection between processes
- [ ] Signal handling
- [ ] Chilena Utils (minimal toybox-inspired userspace tools)
- [ ] English comments across all source files

---

## License

MIT

---

*Inspired by [MOROS](https://github.com/vinc/moros) — written independently from scratch.*
