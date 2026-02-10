//! CLI modules.
//!
//! Each module provides an executor for it's respective subsystem:
//! - [`crate::modules::network::run()`]
//! - [`crate::modules::disk::run()`]
//! - [`crate::modules::cpu::run()`]

pub mod network;
pub mod disk;
pub mod cpu;
