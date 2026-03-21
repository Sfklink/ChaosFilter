//! Chaos injectors: pluggable [`ChaosInjector`] implementations (network `tc`/eBPF, cgroup
//! CPU/memory, block `io.max` throttling, cgroup-wide fd limits). The `chaosfilter` binary
//! builds a fixed vector of injectors in `run_plan` (`src/main.rs`), applies whichever are
//! enabled in the plan, then reverts them after the scheduled hold.

pub mod network;
pub mod block_delay;
pub mod filesystem;
pub mod cpu_memory;
mod ebpf;

use crate::plans::Plan;
use anyhow::Result;

/// One unit of chaos: mutates system state in [`ChaosInjector::apply`] and restores it in
/// [`ChaosInjector::revert`] (best effort, idempotent when nothing was applied).
pub trait ChaosInjector {
    fn name(&self) -> &'static str;
    fn apply(&mut self, plan: &Plan) -> Result<()>;
    fn revert(&mut self) -> Result<()>;
}
