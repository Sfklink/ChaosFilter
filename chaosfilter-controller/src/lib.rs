//! Controller orchestration.
//!
//! High-level operations for validating and executing a [`chaosfilter_common::Plan`].
//!
//! This layer coordinates lower-level “injectors” (e.g., `tc`/qdisc, eBPF loaders, etc.)
//! and is responsible for best-effort cleanup (reverting changes when possible)
//!
//! Submodules expose focused controller functionality, such as network qdisc orchestration
//! in [`crate::qdiscs`]

pub mod qdiscs;
pub mod pid_cgroup;

