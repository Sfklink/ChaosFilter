//! Injector abstraction.
//!
//! Injectors are responsible for validating, applying, and reverting a chaos mechanism
//! (e.g. tc netem, eBPF, device-mapper, etc.).
//!
//! Injectors are typically orchestrated by a controller layer, which handles
//! plan lifecycle and ensures best-effort cleanup.

use anyhow::Result;
use chaosfilter_common::Plan;

/// A reversible chaos mechanism.
///
/// Implementations should aim to be:
/// - **Deterministic** (revert returns the system to a known baseline)
/// - **Best-effort safe** (revert is safe even after partial failures)
/// - **Explicit about requirements** (e.g. CAP_NET_ADMIN, root, mounted bpffs)
pub trait Injector {
    /// Returns a stable, human-readable injector identifier.
    ///
    /// # Returns
    /// A static name used for logs/diagnostics (e.g. `"qdisc_netem"`).
    fn name(&self) -> &'static str;

    /// Validates configuration and environment prerequisites before applying chaos.
    ///
    /// # Arguments
    /// * `plan` - Full chaos plan containing injector configuration and targets.
    ///
    /// # Returns
    /// Returns `Ok(())` if validation passes.
    ///
    /// # Side Effects
    /// Implementations may run read-only system checks (e.g. verify iface exists),
    /// but should avoid making persistent modifications.
    ///
    /// # Errors
    /// Returns an error if required configuration is missing or the environment
    /// cannot support this injector (e.g. missing interface, missing cgroup, etc.).
    fn validate(&self, plan: &Plan) -> Result<()>;

    /// Applies chaos according to the plan.
    ///
    /// # Arguments
    /// * `plan` - Full chaos plan containing injector configuration and targets.
    ///
    /// # Returns
    /// Returns `Ok(())` if chaos was applied successfully.
    ///
    /// # Side Effects
    /// Modifies system state (e.g. attaches qdiscs, loads programs, changes scheduler behavior).
    ///
    /// # Requires
    /// Implementations should document required privileges/capabilities
    /// (e.g. CAP_NET_ADMIN, root, specific mounts).
    ///
    /// # Errors
    /// Returns an error if applying chaos fails.
    ///
    /// # Notes
    /// If `apply` returns `Ok(())`, it must be possible for [`Injector::revert`]
    /// to clean up best-effort. If `apply` partially succeeds and then fails,
    /// `revert` should still be safe to call.
    fn apply(&mut self, plan: &Plan) -> Result<()>;

    /// Reverts any applied chaos (best effort).
    ///
    /// # Returns
    /// Returns `Ok(())` if cleanup succeeded or there was nothing to clean up.
    ///
    /// # Side Effects
    /// Attempts to restore system state (e.g. remove qdiscs, detach programs).
    ///
    /// # Errors
    /// Returns an error if cleanup fails. Callers may treat cleanup as best-effort
    /// depending on the context.
    ///
    /// # Notes
    /// Implementations should aim for idempotency: calling `revert` multiple times
    /// should not cause harm.
    fn revert(&mut self) -> Result<()>;
}
