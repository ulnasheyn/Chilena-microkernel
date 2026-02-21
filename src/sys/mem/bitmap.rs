//! Bitmap Frame Allocator
//!
//! Tracks free/used physical frames using a 64-bit bitmap.
//! Each bit represents one 4 KB frame.

use bootloader::bootinfo::{MemoryMap, MemoryRegionType};
use bit_field::BitField;
use core::{cmp, slice};
use spin::{Mutex, Once};
use x86_64::structures::paging::{FrameAllocator, FrameDeallocator, PhysFrame, Size4KiB};
use x86_64::PhysAddr;

// ---------------------------------------------------------------------------
// Usable physical memory region
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
struct MemRegion {
    first_frame: PhysFrame,
    frame_count: usize,
}

impl MemRegion {
    fn new(start: u64, end: u64) -> Self {
        let first_frame: PhysFrame<Size4KiB> = PhysFrame::containing_address(PhysAddr::new(start));
        let last_frame:  PhysFrame<Size4KiB> = PhysFrame::containing_address(PhysAddr::new(end - 1));
        let count = ((last_frame.start_address() - first_frame.start_address()) / 4096) as usize + 1;
        Self { first_frame, frame_count: count }
    }

    fn last_frame(&self) -> PhysFrame {
        self.first_frame + (self.frame_count - 1) as u64
    }

    fn contains(&self, f: PhysFrame) -> bool {
        self.first_frame <= f && f <= self.last_frame()
    }

    fn index_of(&self, f: PhysFrame) -> usize {
        ((f.start_address() - self.first_frame.start_address()) / 4096) as usize
    }
}

// ---------------------------------------------------------------------------
// BitmapAllocator
// ---------------------------------------------------------------------------

const MAX_REGIONS: usize = 32;

pub struct BitmapAllocator {
    bitmap:     &'static mut [u64],
    next_hint:  usize,
    regions:    [Option<MemRegion>; MAX_REGIONS],
    n_regions:  usize,
    n_frames:   usize,
}

impl BitmapAllocator {
    pub fn build(memory_map: &'static MemoryMap) -> Self {
        // Count total usable frames
        let total_frames: usize = memory_map.iter()
            .filter(|r| r.region_type == MemoryRegionType::Usable)
            .map(|r| ((r.range.end_addr() - r.range.start_addr()) / 4096) as usize)
            .sum();

        let bitmap_bytes = ((total_frames + 63) / 64) * 8;

        let mut alloc = Self {
            bitmap:    &mut [],
            next_hint: 0,
            regions:   [None; MAX_REGIONS],
            n_regions: 0,
            n_frames:  0,
        };

        let mut bitmap_placed = false;

        for region in memory_map.iter() {
            if region.region_type != MemoryRegionType::Usable {
                continue;
            }

            let rstart = region.range.start_addr();
            let rend   = region.range.end_addr();
            let rsize  = (rend - rstart) as usize;

            // Place bitmap in the first region large enough
            let (usable_start, usable_end) = if !bitmap_placed && rsize >= bitmap_bytes {
                bitmap_placed = true;

                let vaddr = super::phys_to_virt(PhysAddr::new(rstart));
                let ptr   = vaddr.as_mut_ptr::<u64>();
                let len   = bitmap_bytes / 8;
                unsafe {
                    alloc.bitmap = slice::from_raw_parts_mut(ptr, len);
                    alloc.bitmap.fill(0);
                }

                let after = rstart + bitmap_bytes as u64;
                if after >= rend { continue; }
                (after, rend)
            } else {
                (rstart, rend)
            };

            if usable_end - usable_start < 4096 { continue; }
            if alloc.n_regions >= MAX_REGIONS { break; }

            let r = MemRegion::new(usable_start, usable_end);
            alloc.regions[alloc.n_regions] = Some(r);
            alloc.n_regions += 1;
            alloc.n_frames  += r.frame_count;
        }

        alloc
    }

    fn frame_at_index(&self, idx: usize) -> Option<PhysFrame> {
        if idx >= self.n_frames { return None; }
        let mut base = 0;
        for i in 0..self.n_regions {
            if let Some(r) = self.regions[i] {
                if idx < base + r.frame_count {
                    return Some(r.first_frame + (idx - base) as u64);
                }
                base += r.frame_count;
            }
        }
        None
    }

    fn index_of_frame(&self, frame: PhysFrame) -> Option<usize> {
        let mut base = 0;
        for i in 0..self.n_regions {
            if let Some(r) = self.regions[i] {
                if r.contains(frame) {
                    return Some(base + r.index_of(frame));
                }
                base += r.frame_count;
            }
        }
        None
    }

    fn is_used(&self, idx: usize) -> bool {
        self.bitmap[idx / 64].get_bit(idx % 64)
    }

    fn set_used(&mut self, idx: usize, used: bool) {
        self.bitmap[idx / 64].set_bit(idx % 64, used);
    }
}

unsafe impl FrameAllocator<Size4KiB> for BitmapAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        for i in 0..self.n_frames {
            let idx = (self.next_hint + i) % self.n_frames;
            if !self.is_used(idx) {
                self.set_used(idx, true);
                self.next_hint = idx + 1;
                return self.frame_at_index(idx);
            }
        }
        None
    }
}

impl FrameDeallocator<Size4KiB> for BitmapAllocator {
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>) {
        if let Some(idx) = self.index_of_frame(frame) {
            if self.is_used(idx) {
                self.set_used(idx, false);
                self.next_hint = cmp::min(self.next_hint, idx);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Global singleton
// ---------------------------------------------------------------------------

static ALLOCATOR: Once<Mutex<BitmapAllocator>> = Once::new();

pub fn init_frame_allocator(memory_map: &'static MemoryMap) {
    ALLOCATOR.call_once(|| Mutex::new(BitmapAllocator::build(memory_map)));
}

pub type FrameAllocatorHandle<'a> = spin::MutexGuard<'a, BitmapAllocator>;

pub fn with_frame_allocator<F, R>(f: F) -> R
where
    F: FnOnce(&mut BitmapAllocator) -> R,
{
    f(&mut ALLOCATOR.get().expect("frame allocator not ready").lock())
}
