//! info — display Chilena system information

use crate::sys;

pub fn run() {
    println!("=== Chilena System Info ===");
    println!("Kernel  : Chilena v{}", crate::VERSION);
    println!("Uptime  : {:.3} seconds", sys::clk::uptime_secs());
    println!("Date    : {}", sys::clk::date_string());
    println!("Memory  : {} MB total, {} MB free",
        sys::mem::total_memory() >> 20,
        sys::mem::free_memory()  >> 20,
    );
    println!("CWD     : {}", sys::process::cwd());
    if let Some(user) = sys::process::current_user() {
        println!("User    : {}", user);
    }
    println!("===========================");
}
