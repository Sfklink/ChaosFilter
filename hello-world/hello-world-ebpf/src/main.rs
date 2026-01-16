#![no_std]
#![no_main]

use aya_ebpf::{macros::tracepoint, programs::TracePointContext};
use aya_log_ebpf::info;

#[tracepoint]
pub fn hello_world(ctx: TracePointContext) -> u32 {
    match try_hello_world(ctx) {
        Ok(ret) => ret,
        Err(ret) => ret,
    }
}

fn try_hello_world(ctx: TracePointContext) -> Result<u32, u32> {
    info!(&ctx, "Hello, World!");
    Ok(0)
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[unsafe(link_section = "license")]
#[unsafe(no_mangle)]
static LICENSE: [u8; 13] = *b"Dual MIT/GPL\0";
