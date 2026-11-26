//! What a sandbox's network reaches, as numbered plans its engine holds it
//! to, and the one rule every engine applies them by.
//!
//! ```text
//!   enforce(plan) ─► refuse when an older plan than the one in force
//!                 ─► apply ─► read back until the mechanism holds `plan`
//! ```
//!
//! The mechanism's own read-back decides what is in force, never a value an
//! engine remembers, so a build returns, and a resumed sandbox serves, only
//! once the kernel holds it to its run's plan.

use std::time::Duration;

use backon::{ExponentialBuilder, Retryable as _};

use crate::error::{EgressRefusal, Result, egress_refused};
use crate::network::Network;

/// The first wait between two read-backs; each later one doubles, until the
/// caller's budget runs out.
const FIRST_READ_BACK: Duration = Duration::from_millis(1);

/// A plan's number: a later plan carries a larger one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Generation(pub u64);

/// What one run's sandbox may reach, numbered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EgressPlan<'a> {
    /// Its number.
    pub generation: Generation,
    /// What it reaches.
    pub network: Network<'a>,
}

/// What holds a sandbox to a plan, read back from its own mechanism.
#[async_trait::async_trait]
pub trait Enforcer: Send + Sync {
    /// Holds the sandbox to `plan` from now on.
    ///
    /// # Errors
    /// The mechanism refused the change; what it held before still holds.
    async fn apply(&mut self, plan: EgressPlan<'_>) -> Result<()>;

    /// The plan the mechanism holds the sandbox to now, read back; none when
    /// what it holds is no plan it was given.
    ///
    /// # Errors
    /// The mechanism could not be read.
    async fn in_force(&self) -> Result<Option<Generation>>;
}

/// Holds `enforcer` to `plan`: refused when an older plan than the one in
/// force, otherwise applied, and returned once the mechanism reads `plan`
/// back in force, within `within`.
///
/// # Errors
/// The plan is stale, the mechanism refused it, or it never read back.
pub async fn enforce(
    enforcer: &mut dyn Enforcer,
    plan: EgressPlan<'_>,
    within: Duration,
) -> Result<()> {
    if let Some(current) = enforcer.in_force().await?
        && plan.generation < current
    {
        return Err(egress_refused(EgressRefusal::Stale {
            asked: plan.generation.0,
            in_force: current.0,
        }));
    }
    enforcer.apply(plan).await?;
    until_in_force(enforcer, plan.generation, within).await
}

/// Returns once `enforcer` reads `generation` back in force, reading again
/// with a doubling wait until `within` has passed.
///
/// # Errors
/// The mechanism could not be read, or never held `generation` in time.
pub async fn until_in_force(
    enforcer: &dyn Enforcer,
    generation: Generation,
    within: Duration,
) -> Result<()> {
    let read = async || match enforcer.in_force().await? {
        Some(held) if held == generation => Ok(()),
        _other => Err(egress_refused(EgressRefusal::NotInForce {
            asked: generation.0,
        })),
    };
    read.retry(
        ExponentialBuilder::default()
            .with_min_delay(FIRST_READ_BACK)
            .with_total_delay(Some(within))
            .without_max_times(),
    )
    .when(|failure| {
        matches!(
            failure.egress_refusal(),
            Some(EgressRefusal::NotInForce { .. })
        )
    })
    .await
}

#[cfg(test)]
mod tests;
