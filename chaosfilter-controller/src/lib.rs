//! Controller orchestration.
//!
//! High-level operations for validating and executing a [`chaosfilter_common::Plan`].
//!
//! This layer coordinates lower-level “injectors” (e.g., `tc`/qdisc, eBPF loaders, etc.)
//! and is responsible for best-effort cleanup (reverting changes when possible)
//!
//! Submodules expose focused controller functionality, such as network qdisc orchestration
//! in [`crate::qdiscs`]

use anyhow::Result;
use chaosfilter_common::{Plan, validate_plan};

pub mod qdiscs;
pub mod pid_cgroup;

pub fn run_plan(plan: &Plan) -> Result<()> {
    validate_plan(plan)?;

    // Each module should early-return Ok(()) when its injector is disabled.
    pid_cgroup::run_plan(plan)?;
    qdiscs::run_plan(plan)?;

    Ok(())
}