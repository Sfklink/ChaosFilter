//! CLI subsystem modules.
//!
//! This module defines the primary subsystem entry points used by the
//! interactive CLI runner.
//!
//! Each submodule exposes a `run()` function that serves as the
//! execution entry point for that subsystem:
//!
//! - [`crate::modules::network::run`]
//! - [`crate::modules::disk::run`]
//! - [`crate::modules::cpu::run`]
//!
//! These modules are invoked by the central dispatcher
//! in [`crate::cli::router`].

pub mod network;
pub mod disk;
pub mod cpu;
