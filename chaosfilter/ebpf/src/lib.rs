#![no_std]
#![no_main]

use aya_ebpf::macros::tracepoint;
use aya_ebpf::programs::tracepoint::TracePointContext;

// --- REQUIRED eBPF LICENSE SECTION ---
#[no_mangle]
#[link_section = "license"]
static LICENSE: &[u8] = b"GPL\0";
// ------------------------------------

#[tracepoint(category = "sched", name = "chaosfilter_noop")]
pub fn chaosfilter_noop(ctx: TracePointContext) -> u32 {
    let _ = ctx;
    0
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
