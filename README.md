# Chilena Kernel

A minimalist x86_64 kernel written in **Rust** (`no_std`).

Inspired by the design philosophy of MOROS — particularly the "everything is a handle" concept, shared use of GDT, IDT, and int 0x80 syscall convention — but written from scratch with a different architecture. Chilena is not a fork or derivative of MOROS; every module, function, and structure was written independently. Compared to MOROS which features a disk-based filesystem, TCP/UDP networking, and built-in apps like a Lisp interpreter and chess, Chilena is intentionally minimal: an in-memory filesystem, 16 syscalls, and a lightweight shell. Think of MOROS as a mature kernel, and Chilena as a newborn — same foundation, different soul.

---

## Architecture

```
src/
├── main.rs          ← entry point (boot → init → shell)
├── lib.rs           ← root library, global macros
├── sys/             ← KERNEL LAYER
│   ├── gdt.rs       ← Global Descriptor Table
│   ├── idt.rs       ← Interrupt Descriptor Table + syscall gate
│   ├── pic.rs       ← 8259 PIC
│   ├── mem/         ← Memory management
│   │   ├── bitmap.rs   ← physical frame allocator
│   │   ├── paging.rs   ← x86_64 page table
│   │   └── heap.rs     ← kernel heap (linked_list_allocator)
│   ├── process.rs   ← process table, context switch, ELF loader
│   ├── syscall/     ← syscall dispatcher + numbers + services
│   ├── fs/          ← in-memory VFS
│   ├── clk/         ← PIT timer + RTC
│   ├── console.rs   ← stdin buffer + output
│   ├── keyboard.rs  ← PS/2 driver
│   ├── serial.rs    ← UART 16550
│   ├── vga/         ← VGA text mode 80×25
│   ├── cpu.rs       ← CPUID info
│   └── acpi.rs      ← power management
├── api/             ← API LAYER (kernel ↔ userspace bridge)
│   ├── process.rs   ← ExitCode, exit()
│   ├── syscall.rs   ← ergonomic syscall wrappers
│   ├── console.rs   ← Style (ANSI colors)
│   └── io.rs        ← read/write helpers
└── usr/             ← USERSPACE LAYER
    ├── shell.rs     ← interactive shell
    ├── help.rs      ← help command
    └── info.rs      ← system info
```

---

## Syscalls

Chilena has **16** clean and minimalist syscalls:

| No   | Name    | Function                      |
|------|---------|-------------------------------|
| 0x01 | EXIT    | Exit a process                |
| 0x02 | SPAWN   | Create a new process from ELF |
| 0x03 | READ    | Read from a handle            |
| 0x04 | WRITE   | Write to a handle             |
| 0x05 | OPEN    | Open a file/device            |
| 0x06 | CLOSE   | Close a handle                |
| 0x07 | STAT    | File metadata                 |
| 0x08 | DUP     | Duplicate a handle            |
| 0x09 | REMOVE  | Delete a file                 |
| 0x0A | HALT    | Halt/reboot the system        |
| 0x0B | SLEEP   | Sleep for N seconds           |
| 0x0C | POLL    | Check I/O readiness           |
| 0x0D | ALLOC   | Allocate userspace memory     |
| 0x0E | FREE    | Free memory                   |
| 0x0F | KIND    | Handle type                   |

Called via `int 0x80` with System V ABI convention.

---

## Build & Run

### Prerequisites

- Rust nightly
- `cargo-bootimage`
- QEMU

### Install tools

```bash
rustup override set nightly
rustup component add rust-src llvm-tools-preview
cargo install bootimage
```

### Build and run

```bash
make run
```

---

## Shell Commands

| Command         | Function                      |
|-----------------|-------------------------------|
| `help`          | Show list of commands         |
| `info`          | System info (RAM, uptime, etc)|
| `echo [text]`   | Print text                    |
| `clear`         | Clear the screen              |
| `cd [path]`     | Change directory              |
| `ls`            | List files                    |
| `cat [file]`    | Show file contents            |
| `write [f] [t]` | Write text to file            |
| `reboot`        | Restart the system            |
| `halt`          | Shutdown the system           |
| `exit`          | Exit the shell                |

---

## License

MIT
