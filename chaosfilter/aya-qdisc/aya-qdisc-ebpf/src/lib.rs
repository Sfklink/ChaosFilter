#![no_std]

// This file exists to enable the library target.

use core::panic::PanicInfo;

/// Required for no_std eBPF programs.
/// Panics must never unwind in eBPF, so we just loop forever.
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
