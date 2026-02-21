//! Process Manager for Chilena
//!
//! Manages the process table, I/O handles, context switching,
//! and loading ELF binaries into userspace memory.

use crate::api::process::ExitCode;
use crate::sys;
use crate::sys::console::Console;
use crate::sys::fs::{Resource, Device};
use crate::sys::gdt::GDT;
use crate::sys::ipc::{BlockState, Message};
use crate::sys::mem::{phys_mem_offset, with_frame_allocator};

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use core::alloc::{GlobalAlloc, Layout};
use core::arch::asm;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use lazy_static::lazy_static;
use linked_list_allocator::LockedHeap;
use object::{Object, ObjectSegment};
use spin::RwLock;
use x86_64::registers::control::Cr3;
use x86_64::structures::idt::InterruptStackFrameValue;
use x86_64::structures::paging::{
    FrameAllocator, FrameDeallocator, OffsetPageTable, PageTable,
    PageTableFlags, PhysFrame, Translate, mapper::TranslateResult,
};
use x86_64::VirtAddr;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const BIN_MAGIC: [u8; 4] = [0x7F, b'C', b'H', b'N']; // Chilena flat binary

pub const MAX_HANDLES:  usize = 64;
pub const MAX_PROCS:    usize = 8;
pub const MAX_PROC_MEM: usize = 10 << 20; // 10 MB per process

/// Start address of userspace (must be above kernel)
const USER_BASE: u64 = 0x0080_0000;

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------

static PROC_CODE_BASE: AtomicU64    = AtomicU64::new(0);
pub static CURRENT_PID: AtomicUsize = AtomicUsize::new(0);
pub static NEXT_PID:    AtomicUsize = AtomicUsize::new(1);

lazy_static! {
    pub static ref PROC_TABLE: RwLock<[Box<Process>; MAX_PROCS]> = {
        RwLock::new([(); MAX_PROCS].map(|_| Box::new(Process::new())))
    };
}

pub fn set_proc_code_base(addr: u64) {
    PROC_CODE_BASE.store(addr, Ordering::SeqCst);
}

// ---------------------------------------------------------------------------
// Register state (System V ABI scratch registers)
// ---------------------------------------------------------------------------

#[repr(align(8), C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuRegisters {
    pub r11: usize,
    pub r10: usize,
    pub r9:  usize,
    pub r8:  usize,
    pub rdi: usize,
    pub rsi: usize,
    pub rdx: usize,
    pub rcx: usize,
    pub rax: usize,
}

// ---------------------------------------------------------------------------
// Process data (env, cwd, handles)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ProcData {
    pub env:     BTreeMap<String, String>,
    pub cwd:     String,
    pub user:    Option<String>,
    pub handles: [Option<Box<Resource>>; MAX_HANDLES],
}

impl ProcData {
    pub fn new(cwd: &str, user: Option<&str>) -> Self {
        let mut handles = [(); MAX_HANDLES].map(|_| None);

        // stdin=0, stdout=1, stderr=2, null=3
        handles[0] = Some(Box::new(Resource::Device(Device::Console(Console::new()))));
        handles[1] = Some(Box::new(Resource::Device(Device::Console(Console::new()))));
        handles[2] = Some(Box::new(Resource::Device(Device::Console(Console::new()))));
        handles[3] = Some(Box::new(Resource::Device(Device::Null)));

        Self {
            env:  BTreeMap::new(),
            cwd:  cwd.to_string(),
            user: user.map(String::from),
            handles,
        }
    }
}

// ---------------------------------------------------------------------------
// Process API — access current process state
// ---------------------------------------------------------------------------

pub fn current_pid() -> usize       { CURRENT_PID.load(Ordering::SeqCst) }
pub fn set_pid(id: usize)           { CURRENT_PID.store(id, Ordering::SeqCst); }

pub fn cwd() -> String {
    PROC_TABLE.read()[current_pid()].data.cwd.clone()
}

pub fn set_cwd(path: &str) {
    PROC_TABLE.write()[current_pid()].data.cwd = path.to_string();
}

pub fn env_var(key: &str) -> Option<String> {
    PROC_TABLE.read()[current_pid()].data.env.get(key).cloned()
}

pub fn set_env_var(key: &str, val: &str) {
    PROC_TABLE.write()[current_pid()].data.env.insert(key.into(), val.into());
}

pub fn current_user() -> Option<String> {
    PROC_TABLE.read()[current_pid()].data.user.clone()
}

// ---------------------------------------------------------------------------
// Handle management
// ---------------------------------------------------------------------------

pub fn alloc_handle(res: Resource) -> Result<usize, ()> {
    let mut table = PROC_TABLE.write();
    let proc = &mut table[current_pid()];
    for i in 4..MAX_HANDLES {
        if proc.data.handles[i].is_none() {
            proc.data.handles[i] = Some(Box::new(res));
            return Ok(i);
        }
    }
    Err(())
}

pub fn get_handle(h: usize) -> Option<Box<Resource>> {
    PROC_TABLE.read()[current_pid()].data.handles[h].clone()
}

pub fn update_handle(h: usize, res: Resource) {
    PROC_TABLE.write()[current_pid()].data.handles[h] = Some(Box::new(res));
}

pub fn free_handle(h: usize) {
    PROC_TABLE.write()[current_pid()].data.handles[h] = None;
}

// ---------------------------------------------------------------------------
// Saved registers & stack frame (for spawn/exit context switch)
// ---------------------------------------------------------------------------

pub fn saved_registers() -> CpuRegisters {
    PROC_TABLE.read()[current_pid()].saved_regs
}

pub fn save_registers(r: CpuRegisters) {
    PROC_TABLE.write()[current_pid()].saved_regs = r;
}

pub fn saved_stack_frame() -> Option<InterruptStackFrameValue> {
    PROC_TABLE.read()[current_pid()].stack_frame
}

pub fn save_stack_frame(sf: InterruptStackFrameValue) {
    PROC_TABLE.write()[current_pid()].stack_frame = Some(sf);
}

// ---------------------------------------------------------------------------
// Memory address helpers
// ---------------------------------------------------------------------------

pub fn code_base() -> u64 {
    PROC_TABLE.read()[current_pid()].code_base
}

/// Convert a userspace pointer (possibly relative) to an absolute kernel address
pub fn resolve_addr(addr: u64) -> *mut u8 {
    let base = code_base();
    if addr < base { (base + addr) as *mut u8 } else { addr as *mut u8 }
}

pub fn is_user_addr(addr: u64) -> bool {
    USER_BASE <= addr && addr <= USER_BASE + MAX_PROC_MEM as u64
}

// ---------------------------------------------------------------------------
// Per-process memory allocation
// ---------------------------------------------------------------------------

pub unsafe fn user_alloc(layout: Layout) -> *mut u8 {
    PROC_TABLE.read()[current_pid()].allocator.alloc(layout)
}

pub unsafe fn user_free(ptr: *mut u8, layout: Layout) {
    let table = PROC_TABLE.read();
    let proc  = &table[current_pid()];
    let bot   = proc.allocator.lock().bottom();
    let top   = proc.allocator.lock().top();
    if (bot as u64) <= ptr as u64 && ptr < top {
        proc.allocator.dealloc(ptr, layout);
    }
}

// ---------------------------------------------------------------------------
// Per-process page table
// ---------------------------------------------------------------------------

unsafe fn current_page_table_frame() -> PhysFrame {
    PROC_TABLE.read()[current_pid()].pt_frame
}

pub unsafe fn page_table() -> &'static mut PageTable {
    sys::mem::create_page_table_from_frame(current_page_table_frame())
}

// ---------------------------------------------------------------------------
// Process termination
// ---------------------------------------------------------------------------

pub fn terminate() {
    let table = PROC_TABLE.read();
    let proc  = &table[current_pid()];
    let parent_id = proc.parent_id;

    if NEXT_PID.load(Ordering::SeqCst) > 0 {
        NEXT_PID.fetch_sub(1, Ordering::SeqCst);
    }
    set_pid(parent_id);

    proc.release_pages();
    unsafe {
        let (_, flags) = Cr3::read();
        Cr3::write(current_page_table_frame(), flags);
        with_frame_allocator(|fa| {
            fa.deallocate_frame(proc.pt_frame);
        });
    }
}

pub fn power_off_hook() {
    terminate();
    sys::acpi::power_off();
}

// ---------------------------------------------------------------------------
// Process struct
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Process {
    pub id:          usize,
    pub parent_id:   usize,
    pub code_base:   u64,
    pub stack_base:  u64,
    pub entry_point: u64,
    pub pt_frame:    PhysFrame,
    pub stack_frame: Option<InterruptStackFrameValue>,
    pub saved_regs:  CpuRegisters,
    pub data:        ProcData,
    pub allocator:   Arc<LockedHeap>,
    /// IPC mailbox — single incoming message slot
    pub mailbox:     Option<Message>,
    /// Process block state (Running / WaitingSend / WaitingRecv)
    pub block:       BlockState,
}

impl Process {
    pub fn new() -> Self {
        Self {
            id:          0,
            parent_id:   0,
            code_base:   0,
            stack_base:  0,
            entry_point: 0,
            pt_frame:    Cr3::read().0,
            stack_frame: None,
            saved_regs:  CpuRegisters::default(),
            data:        ProcData::new("/", None),
            allocator:   Arc::new(LockedHeap::empty()),
            mailbox:     None,
            block:       BlockState::Running,
        }
    }

    pub fn spawn(bin: &[u8], args_ptr: usize, args_len: usize) -> Result<(), ExitCode> {
        if let Ok(id) = Self::create(bin) {
            let proc = PROC_TABLE.read()[id].clone();
            proc.exec(args_ptr, args_len);
            unreachable!();
        }
        Err(ExitCode::ExecError)
    }

    fn create(bin: &[u8]) -> Result<usize, ()> {
        if NEXT_PID.load(Ordering::SeqCst) >= MAX_PROCS {
            return Err(());
        }

        // Allocate frame for new process page table
        let pt_frame = with_frame_allocator(|fa| {
            fa.allocate_frame().expect("could not allocate frame for page table")
        });

        let new_pt     = unsafe { sys::mem::create_page_table_from_frame(pt_frame) };
        let kernel_pt  = unsafe { sys::mem::active_page_table() };

        // Copy entire kernel page table to new process
        for (dst, src) in new_pt.iter_mut().zip(kernel_pt.iter()) {
            *dst = src.clone();
        }

        let mut mapper = unsafe {
            OffsetPageTable::new(new_pt, VirtAddr::new(phys_mem_offset()))
        };

        let code_base  = PROC_CODE_BASE.fetch_add(MAX_PROC_MEM as u64, Ordering::SeqCst);
        let stack_base = code_base + MAX_PROC_MEM as u64 - 4096;
        let mut entry_point = 0u64;

        // Load ELF or flat binary
        if bin.get(0..4) == Some(&ELF_MAGIC) {
            if let Ok(obj) = object::File::parse(bin) {
                entry_point = obj.entry();
                for seg in obj.segments() {
                    if let Ok(data) = seg.data() {
                        let addr = code_base + seg.address();
                        let size = seg.size() as usize;
                        Self::load_segment(&mut mapper, addr, size, data)?;
                    }
                }
            }
        } else if bin.get(0..4) == Some(&BIN_MAGIC) {
            Self::load_segment(&mut mapper, code_base, bin.len() - 4, &bin[4..])?;
        } else {
            return Err(());
        }

        let parent = PROC_TABLE.read()[current_pid()].clone();
        let id     = NEXT_PID.fetch_add(1, Ordering::SeqCst);

        let proc = Process {
            id,
            parent_id:   parent.id,
            code_base,
            stack_base,
            entry_point,
            pt_frame,
            data:        parent.data.clone(),
            stack_frame: parent.stack_frame,
            saved_regs:  parent.saved_regs,
            allocator:   Arc::new(LockedHeap::empty()),
            mailbox:     None,
            block:       BlockState::Running,
        };

        PROC_TABLE.write()[id] = Box::new(proc);
        Ok(id)
    }

    fn exec(&self, args_ptr: usize, args_len: usize) {
        let pt  = unsafe { page_table() };
        let mut mapper = unsafe {
            OffsetPageTable::new(pt, VirtAddr::new(phys_mem_offset()))
        };

        // Copy arguments into process memory
        let args_base = self.code_base + (self.stack_base - self.code_base) / 2;
        sys::mem::map_page(&mut mapper, args_base, 1).expect("args alloc");

        let args: &[&str] = unsafe {
            let ptr = resolve_addr(args_ptr as u64) as usize;
            core::slice::from_raw_parts(ptr as *const &str, args_len)
        };

        let mut cursor = args_base;
        let mut str_slices = alloc::vec::Vec::new();

        for arg in args {
            let dst = cursor as *mut u8;
            cursor += arg.len() as u64;
            unsafe {
                let s = core::slice::from_raw_parts_mut(dst, arg.len());
                s.copy_from_slice(arg.as_bytes());
                str_slices.push(core::str::from_utf8_unchecked(s));
            }
        }

        // Align to pointer size
        let align = core::mem::align_of::<&str>() as u64;
        cursor = (cursor + align - 1) & !(align - 1);

        let args_slice_ptr = cursor as *mut &str;
        let final_args: &[&str] = unsafe {
            let s = core::slice::from_raw_parts_mut(args_slice_ptr, str_slices.len());
            s.copy_from_slice(&str_slices);
            s
        };

        let heap_start = cursor + 4096;
        let heap_size  = ((self.stack_base - heap_start) / 2) as usize;
        unsafe {
            self.allocator.lock().init(heap_start as *mut u8, heap_size);
        }

        set_pid(self.id);

        unsafe {
            let (_, flags) = Cr3::read();
            Cr3::write(self.pt_frame, flags);

            asm!(
                "cli",
                "push {ss:r}",    // SS
                "push {rsp:r}",   // RSP
                "push 0x200",     // RFLAGS (IF=1)
                "push {cs:r}",    // CS
                "push {rip:r}",   // RIP
                "iretq",
                ss  = in(reg) GDT.1.u_data.0,
                rsp = in(reg) self.stack_base,
                cs  = in(reg) GDT.1.u_code.0,
                rip = in(reg) self.code_base + self.entry_point,
                in("rdi") final_args.as_ptr(),
                in("rsi") final_args.len(),
            );
        }
    }

    fn mapper(&self) -> OffsetPageTable<'_> {
        let pt = unsafe { sys::mem::create_page_table_from_frame(self.pt_frame) };
        unsafe { OffsetPageTable::new(pt, VirtAddr::new(phys_mem_offset())) }
    }

    fn release_pages(&self) {
        let mut mapper = self.mapper();
        sys::mem::unmap_page(&mut mapper, self.code_base, MAX_PROC_MEM);

        match mapper.translate(VirtAddr::new(USER_BASE)) {
            TranslateResult::Mapped { flags, .. } => {
                if flags.contains(PageTableFlags::USER_ACCESSIBLE) {
                    sys::mem::unmap_page(&mut mapper, USER_BASE, MAX_PROC_MEM);
                }
            }
            _ => {}
        }
    }

    fn load_segment(
        mapper: &mut OffsetPageTable,
        addr:   u64,
        size:   usize,
        data:   &[u8],
    ) -> Result<(), ()> {
        sys::mem::map_page(mapper, addr, size)?;
        unsafe {
            let dst = addr as *mut u8;
            core::ptr::copy_nonoverlapping(data.as_ptr(), dst, data.len());
            if size > data.len() {
                core::ptr::write_bytes(dst.add(data.len()), 0, size - data.len());
            }
        }
        Ok(())
    }
}
