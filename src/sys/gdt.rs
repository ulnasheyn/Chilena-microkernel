//! GDT — Global Descriptor Table
//!
//! Defines kernel and userspace memory segments,
//! and the Task State Segment (TSS) for interrupt stacks.

use core::ptr::addr_of;
use lazy_static::lazy_static;
use x86_64::instructions::segmentation::{CS, DS, Segment};
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

/// Stack size for each IST entry (128 KB)
const IST_STACK_SIZE: usize = 128 * 1024;

/// IST index for each fault type
pub const DOUBLE_FAULT_IST:  u16 = 0;
pub const PAGE_FAULT_IST:    u16 = 1;
pub const GPF_IST:           u16 = 2;

lazy_static! {
    /// Task State Segment — holds stack pointer for privilege switch
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();

        // Ring-0 stack for privilege transition (userspace → kernel)
        tss.privilege_stack_table[0] = {
            static mut STACK: [u8; IST_STACK_SIZE] = [0; IST_STACK_SIZE];
            VirtAddr::from_ptr(addr_of!(STACK)) + IST_STACK_SIZE as u64
        };

        // IST 0: Double Fault
        tss.interrupt_stack_table[DOUBLE_FAULT_IST as usize] = {
            static mut STACK: [u8; IST_STACK_SIZE] = [0; IST_STACK_SIZE];
            VirtAddr::from_ptr(addr_of!(STACK)) + IST_STACK_SIZE as u64
        };

        // IST 1: Page Fault
        tss.interrupt_stack_table[PAGE_FAULT_IST as usize] = {
            static mut STACK: [u8; IST_STACK_SIZE] = [0; IST_STACK_SIZE];
            VirtAddr::from_ptr(addr_of!(STACK)) + IST_STACK_SIZE as u64
        };

        // IST 2: General Protection Fault
        tss.interrupt_stack_table[GPF_IST as usize] = {
            static mut STACK: [u8; IST_STACK_SIZE] = [0; IST_STACK_SIZE];
            VirtAddr::from_ptr(addr_of!(STACK)) + IST_STACK_SIZE as u64
        };

        tss
    };
}

/// Segment selectors used by kernel and userspace
pub struct SegmentSelectors {
    pub tss:       SegmentSelector,
    pub k_code:    SegmentSelector,
    pub k_data:    SegmentSelector,
    pub u_code:    SegmentSelector,
    pub u_data:    SegmentSelector,
}

lazy_static! {
    /// Chilena GDT and segment selectors
    pub static ref GDT: (GlobalDescriptorTable, SegmentSelectors) = {
        let mut gdt = GlobalDescriptorTable::new();

        let tss    = gdt.add_entry(Descriptor::tss_segment(&TSS));
        let k_code = gdt.add_entry(Descriptor::kernel_code_segment());
        let k_data = gdt.add_entry(Descriptor::kernel_data_segment());
        let u_code = gdt.add_entry(Descriptor::user_code_segment());
        let u_data = gdt.add_entry(Descriptor::user_data_segment());

        (gdt, SegmentSelectors { tss, k_code, k_data, u_code, u_data })
    };
}

/// Initialize GDT and load into processor
pub fn init() {
    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.k_code);
        DS::set_reg(GDT.1.k_data);
        load_tss(GDT.1.tss);
    }
}
