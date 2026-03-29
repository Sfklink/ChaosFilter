#![no_std]
//! # ChaosFilter Common
//!
//! This crate contains shared data structures and constants that are used by
//! both the userspace controller (`chaosfilter`) and the in-kernel eBPF
//! programs (`chaosfilter-ebpf`).
//!
//! Because this crate is shared with eBPF programs, it is marked as `#![no_std]`.

/// The firewall mark used by the eBPF classifier to tag packets for chaos.
///
/// When a packet is marked with this value, the userspace `tc` filter will
/// redirect it to the `netem` queuing discipline for fault injection.
pub const CHAOS_MARK: u32 = 1;
