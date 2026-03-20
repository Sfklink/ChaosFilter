pub mod network;
pub mod block_delay;
pub mod filesystem;
pub mod cpu_memory;
mod ebpf;

use crate::plans::Plan;
use anyhow::Result;

/// Common lifecycle for all chaos injectors.
///
/// The caller is responsible for orchestration (apply -> hold -> revert).
pub trait ChaosInjector {
    fn name(&self) -> &'static str;
    fn apply(&mut self, plan: &Plan) -> Result<()>;
    fn revert(&mut self) -> Result<()>;
}
