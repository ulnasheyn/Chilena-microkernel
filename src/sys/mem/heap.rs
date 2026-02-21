use super::with_frame_allocator;
use crate::sys;

use linked_list_allocator::LockedHeap;
use x86_64::structures::paging::{
    mapper::MapToError, FrameAllocator, Mapper,
    Page, PageTableFlags, Size4KiB,
};
use x86_64::VirtAddr;

pub const HEAP_BASE: u64 = 0x4444_4444_0000;
const MAX_HEAP: u64 = 4 << 20; // max 4 MB heap

#[global_allocator]
static KERNEL_HEAP: LockedHeap = LockedHeap::empty();

pub fn init_kernel_heap() -> Result<(), MapToError<Size4KiB>> {
    let mapper = super::mapper();

    // Limit heap to 4 MB maximum
    let total = super::total_memory() as u64;
    let heap_size = (total / 2).min(MAX_HEAP);
    let heap_start = VirtAddr::new(HEAP_BASE);

    sys::process::set_proc_code_base(HEAP_BASE + heap_size);

    let start_page = Page::containing_address(heap_start);
    let end_page   = Page::containing_address(heap_start + heap_size - 1u64);
    let pages      = Page::range_inclusive(start_page, end_page);

    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

    with_frame_allocator(|fa| -> Result<(), MapToError<Size4KiB>> {
        for page in pages {
            let frame = fa.allocate_frame().ok_or(MapToError::FrameAllocationFailed)?;
            unsafe {
                mapper.map_to(page, frame, flags, fa)?.flush();
            }
        }
        Ok(())
    })?;

    unsafe {
        KERNEL_HEAP.lock().init(heap_start.as_mut_ptr(), heap_size as usize);
    }

    Ok(())
}

pub fn heap_size() -> usize { KERNEL_HEAP.lock().size() }
pub fn heap_used() -> usize { KERNEL_HEAP.lock().used() }
pub fn heap_free() -> usize { KERNEL_HEAP.lock().free() }
