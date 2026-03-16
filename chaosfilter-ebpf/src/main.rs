#![no_std]
#![no_main]

use chaosfilter_common::CHAOS_MARK;
use aya_ebpf::{
    bindings::{TC_ACT_OK, TC_ACT_RECLASSIFY},
    helpers::bpf_get_current_cgroup_id,
    macros::{classifier, map},
    maps::HashMap,
    programs::TcContext,
};
use aya_log_ebpf::info;


#[map]
static TARGET_CGROUPS: HashMap<u64, u8> = HashMap::with_max_entries(1024, 0);

#[classifier]
pub fn chaosfilter(ctx: TcContext) -> i32 {
    match try_chaosfilter(ctx) {
        Ok(ret) => ret,
        Err(ret) => ret,
    }
}

fn try_chaosfilter(mut ctx: TcContext) -> Result<i32, i32> {
    let cgroup_id = unsafe { bpf_get_current_cgroup_id() };
    let matched = unsafe { TARGET_CGROUPS.get(&cgroup_id).is_some() };

    if matched {
        info!(&ctx, "matched target cgroup {}", cgroup_id);
        ctx.set_mark(CHAOS_MARK);
        return Ok(TC_ACT_RECLASSIFY);
    }

    Ok(TC_ACT_OK)
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[unsafe(link_section = "license")]
#[unsafe(no_mangle)]
static LICENSE: [u8; 13] = *b"Dual MIT/GPL\0";
