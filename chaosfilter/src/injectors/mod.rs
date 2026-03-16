pub mod memory;
pub mod network;
pub mod filesystem;
pub mod ebpf;

use anyhow::Result;
use crate::plans::Plan;

pub trait Injector {
    fn name(&self) -> &'static str;
    fn apply(&mut self, plan: &Plan) -> Result<()>;
    fn revert(&mut self) -> Result<()>;
    fn validate(&self, plan: &Plan) -> Result<()>;
}

pub fn build_injectors(plan: &Plan) -> Result<Vec<Box<dyn Injector>>> {
    let mut injectors: Vec<Box<dyn Injector>> = Vec::new();

    if plan.injectors.memory_config.enabled {
        injectors.push(Box::new(memory::MemoryConfig::default()));
    }

    if plan.injectors.network_config.enabled {
        injectors.push(Box::new(network::NetworkConfig::default()));
    }

    if plan.injectors.filesystem_config.enabled {
        injectors.push(Box::new(filesystem::FdExhaustConfig::default()));
    }

    Ok(injectors)
}

pub fn validate_injectors(plan: &Plan) -> Result<()> {
    let injectors = build_injectors(plan)?;

    for injector in injectors.iter() {
        injector.validate(plan)?;
    }

    Ok(())
}
