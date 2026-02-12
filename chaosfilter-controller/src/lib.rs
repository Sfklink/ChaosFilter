//! Controller orchestration.
//!
//! High-level operations for validating and running a [`Plan`].
//! This layer coordinates injectors (tc/qdisc, eBPF load, etc.) and ensures
//! best-effort cleanup.

pub mod qdiscs;

