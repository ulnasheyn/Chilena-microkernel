//! CPU — Processor information detection via CPUID

use raw_cpuid::CpuId;

pub fn init() {
    let cpuid = CpuId::new();

    if let Some(v) = cpuid.get_vendor_info() {
        klog!("CPU vendor: {}", v);
    }

    if let Some(brand) = cpuid.get_processor_brand_string() {
        klog!("CPU: {}", brand.as_str().trim());
    }

    if let Some(freq) = cpuid.get_processor_frequency_info() {
        let mhz = freq.processor_base_frequency();
        if mhz > 0 {
            klog!("CPU: {} MHz", mhz);
        }
    }
}
