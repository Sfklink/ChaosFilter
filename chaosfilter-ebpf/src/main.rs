#![no_std]
#![no_main]
//! # ChaosFilter eBPF Classifier
//!
//! This eBPF program is a Traffic Control (TC) classifier. It runs in the kernel
//! for every packet on the egress of a network interface.
//!
//! The program identifies packets belonging to specific cgroups by checking
//! their ID against a pre-populated BPF map. If a match is found, the packet
//! is marked with a special firewall mark (`CHAOS_MARK`), which informs the
//! userspace `tc` subsystem to redirect it for fault injection.

use aya_ebpf::{
    bindings::{TC_ACT_OK, TC_ACT_RECLASSIFY},
    helpers::bpf_get_current_cgroup_id,
    macros::{classifier, map},
    maps::HashMap,
    programs::TcContext,
};
use aya_log_ebpf::info;
use chaosfilter_common::CHAOS_MARK;

/// A BPF map containing the IDs of cgroups targeted for chaos.
///
/// This map is populated by the userspace controller before the eBPF
/// program is attached to an interface.
#[map]
static TARGET_CGROUPS: HashMap<u64, u8> = HashMap::with_max_entries(1024, 0);

/// The primary entry point for the eBPF classifier.
///
/// # Arguments
///
/// * `ctx` - The Traffic Control context for the current packet.
///
/// # Returns
///
/// Returns `TC_ACT_RECLASSIFY` if the packet was marked for chaos,
/// otherwise `TC_ACT_OK`.
#[classifier]
pub fn chaosfilter(ctx: TcContext) -> i32 {
    match try_chaosfilter(ctx) {
        Ok(ret) => ret,
        Err(ret) => ret,
    }
}

/// Internal logic for filtering packets by cgroup ID.
///
/// # Arguments
///
/// * `ctx` - The Traffic Control context.
///
/// # Returns
///
/// Returns `Ok(TC_ACT_RECLASSIFY)` if the packet's cgroup matches a target,
/// otherwise `Ok(TC_ACT_OK)`.
fn try_chaosfilter(ctx: TcContext) -> Result<i32, i32> {
    let cgroup_id = unsafe { bpf_get_current_cgroup_id() };
    info!(&ctx, "egress hit, current cgroup {}", cgroup_id);

    let matched = unsafe { TARGET_CGROUPS.get(&cgroup_id).is_some() };

    if matched {
        info!(&ctx, "matched target cgroup {}", cgroup_id);
        // Mark the packet so the userspace 'fw' filter can pick it up.
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
