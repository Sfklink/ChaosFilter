//Aggregates all testing system domains that the CLI supports

//Each module represents a high-level category of system behavior
//that ChaosFilter can mess with

//Add new domains here then added to the dispatcher when ready

pub mod network;
pub mod disk;
pub mod cpu;
