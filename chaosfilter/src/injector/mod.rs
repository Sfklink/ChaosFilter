use crate::plans::Plan;

pub mod network;
pub mod block_delay;
pub mod filesystem;
pub mod cpu_memory;
mod ebpf;

/// - `apply()` mutates system state and should capture whatever it needs to revert later.
/// - `revert()` restores the baseline state as best-effort.
pub trait ChaosInjector {
    fn name(&self) -> &'static str;
    fn apply(&mut self, plan: &Plan) -> anyhow::Result<()>;
    fn revert(&mut self) -> anyhow::Result<()>;
}