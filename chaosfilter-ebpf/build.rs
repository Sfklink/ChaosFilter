//! # eBPF Build Configuration
//!
//! This script manages the build-time requirements for the eBPF programs.
//! Specifically, it ensures that the BPF linker is available and triggers
//! a rebuild if the linker changes.

use which::which;

/// Building this crate has an undeclared dependency on the `bpf-linker` binary.
/// This file implements a solution to ensure cargo rebuilds the crate whenever the
/// mtime of the `bpf-linker` changes.
fn main() {
    let bpf_linker = which("bpf-linker").expect("bpf-linker not found in PATH");
    println!("cargo:rerun-if-changed={}", bpf_linker.to_str().unwrap());
}
