#![no_std]
//! # ChaosFilter eBPF Library
//!
//! This crate contains the eBPF programs and logic that run in-kernel for
//! ChaosFilter. These programs are compiled to the `bpfel-unknown-none` target
//! and are typically embedded into the userspace controller.
//!
//! Currently, the primary program is a Traffic Control (TC) classifier
//! implemented in the `main.rs` binary target.
