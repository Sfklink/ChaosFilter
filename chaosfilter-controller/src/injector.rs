use anyhow::Result;
use chaosfilter_common::Plan;

/// Simple interface for “a thing that can apply chaos and revert it”.
pub trait Injector {
    fn name(&self) -> &'static str;

    /// Validate config + environment before applying.
    fn validate(&self, plan: &Plan) -> Result<()>;

    /// Apply chaos. Must be reversible by `revert`.
    fn apply(&mut self, plan: &Plan) -> Result<()>;

    /// Best-effort cleanup. Should be safe to call even if apply partially failed.
    fn revert(&mut self) -> Result<()>;
}
