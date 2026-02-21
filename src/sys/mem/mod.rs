//! `sys::mem` — Memory management for Chilena
//!
//! Consists of:
//!   - frame_alloc: physical frame allocation via bitmap
//!   - paging: x86_64 page table manipulation
//!   - heap: global kernel heap (linked_list_allocator)

mod bitmap;
mod heap;
mod paging;

pub use bitmap::{with_frame_allocator, FrameAllocatorHandle};
pub use paging::{map_page, unmap_page, active_page_table, create_page_table_from_frame};

use crate::sys;
use bootloader::bootinfo::{BootInfo, MemoryMap};
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Once;
use x86_64::structures::paging::{OffsetPageTable, Translate};
use x86_64::{PhysAddr, VirtAddr};

// ---------------------------------------------------------------------------
// Global state — initialized once during boot
// ---------------------------------------------------------------------------

#[allow(static_mut_refs)]
static mut MAPPER: Once<OffsetPageTable<'static>> = Once::new();

static PHYS_OFFSET:  Once<u64>         = Once::new();
static MEM_MAP:      Once<&MemoryMap>  = Once::new();
static TOTAL_BYTES:  AtomicUsize       = AtomicUsize::new(0);

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

pub fn init(boot_info: &'static BootInfo) {
    // Temporarily mask keyboard to avoid interference during allocation
    sys::idt::set_irq_mask(1);

    let mut total = 0usize;
    let mut prev_end = 0u64;

    for region in boot_info.memory_map.iter() {
        let start = region.range.start_addr();
        let end   = region.range.end_addr();
        let size  = end - start;

        if start > prev_end {
            klog!("MEM [{:#016X}-{:#016X}] (gap)", prev_end, start - 1);
        }
        klog!("MEM [{:#016X}-{:#016X}] {:?}", start, end - 1, region.region_type);
        total += size as usize;
        prev_end = end;
    }

    klog!("RAM {} MB total", total >> 20);
    TOTAL_BYTES.store(total, Ordering::Relaxed);

    PHYS_OFFSET.call_once(|| boot_info.physical_memory_offset);
    MEM_MAP.call_once(|| &boot_info.memory_map);

    #[allow(static_mut_refs)]
    unsafe {
        MAPPER.call_once(|| {
            OffsetPageTable::new(
                paging::active_page_table(),
                VirtAddr::new(boot_info.physical_memory_offset),
            )
        });
    }

    bitmap::init_frame_allocator(&boot_info.memory_map);
    heap::init_kernel_heap().expect("heap init failed");

    sys::idt::clear_irq_mask(1);
}

// ---------------------------------------------------------------------------
// Public helpers
// ---------------------------------------------------------------------------

pub fn phys_mem_offset() -> u64 {
    unsafe { *PHYS_OFFSET.get_unchecked() }
}

pub fn mapper() -> &'static mut OffsetPageTable<'static> {
    #[allow(static_mut_refs)]
    unsafe { MAPPER.get_mut_unchecked() }
}

pub fn total_memory() -> usize {
    TOTAL_BYTES.load(Ordering::Relaxed)
}

pub fn used_memory() -> usize {
    (total_memory() - heap::heap_size()) + heap::heap_used()
}

pub fn free_memory() -> usize {
    heap::heap_free()
}

pub fn phys_to_virt(phys: PhysAddr) -> VirtAddr {
    VirtAddr::new(phys.as_u64() + phys_mem_offset())
}

pub fn virt_to_phys(virt: VirtAddr) -> Option<PhysAddr> {
    mapper().translate_addr(virt)
}
