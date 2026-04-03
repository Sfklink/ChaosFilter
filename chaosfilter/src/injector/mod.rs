//! # Fault Injector Abstractions
//!
//! This module defines the [`ChaosInjector`] trait, which all fault injection
//! mechanisms must implement. It also declares the submodules for each
//! specific injector type.

pub mod network;
pub mod filesystem;
pub mod cpu_memory;

use crate::plans::Plan;
use anyhow::Result;

/// A trait for implementing chaos fault injection.
///
/// Each injector is responsible for a specific type of fault (e.g., network, memory).
pub trait ChaosInjector {
    /// Returns a human-readable name for the injector.
    ///
    /// This name is used for logging and error reporting.
    fn name(&self) -> &'static str;

    /// Applies the fault injection based on the provided [`Plan`].
    ///
    /// # Arguments
    ///
    /// * `plan` - The chaos plan containing the configuration for this injector.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the fault was successfully injected.
    ///
    /// # Behavior
    ///
    /// This method should perform all necessary system modifications to initiate
    /// the requested chaos state.
    ///
    /// # Errors
    ///
    /// Returns an error if the injection fails (e.g., due to permission issues or missing dependencies).
    fn apply(&mut self, plan: Plan) -> Result<()>;

    /// Reverts the fault injection, returning the system to its original state.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the system was successfully restored.
    ///
    /// # Behavior
    ///
    /// This method must undo all changes made during the apply phase. It should be
    /// designed to be as idempotent and robust as possible.
    ///
    /// # Errors
    ///
    /// Returns an error if the reversion fails.
    fn revert(&mut self) -> Result<()>;
}

impl dyn ChaosInjector {
    /// Helper to get the name of a boxed [`ChaosInjector`].
    pub fn name(injector: &dyn ChaosInjector) -> &'static str {
        injector.name()
    }
}
