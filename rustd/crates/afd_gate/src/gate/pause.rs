//! Stopping a fleet, when the gate decides no further work of its may run.
//!
//! # Two auto-kill paths, one stop
//!
//! A tripped anomaly rule and an `auto_kill` gate rule reach the same place: the
//! fleet stops, and the event that triggered it ends. They differ only in what
//! an operator is told, which is why the trigger travels as a value rather than
//! as two copies of this function.
//!
//! # The readiness clear runs only if the pause committed
//!
//! A paused fleet leaves the candidate query's reach — that query filters
//! `status = 'active'` — so the poll-site clear can never remove its readiness
//! field afterwards. It is cleared HERE instead, after the pause commits. On a
//! failed update the clear is skipped, and that direction matters: the fleet is
//! still active, so its mark still names live work, and clearing it would hide
//! a running fleet from every later poll.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;

use crate::error::{Result, query};
use crate::gate::store::Gates;
use super::sql;

/// Statement name, for the context a query failure carries.
const CONTEXT_PAUSE: &str = "fleet pause";

/// A fleet was stopped by the gate.
const EVENT_FLEET_PAUSED: &str = "gate_fleet_paused";

/// The readiness mark outlived the pause that should have cleared it.
const EVENT_READY_STALE: &str = "gate_ready_clear_failed";

/// What stopped the fleet.
///
/// Two triggers and not a boolean: an operator reading the line wants to know
/// whether a human's policy said stop or a runaway pattern did, and those lead
/// to different runbooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// One action repeated past its threshold.
    Anomaly,
    /// A gate rule's behavior is `auto_kill`.
    Policy,
}

impl Trigger {
    /// The trigger's name, for the line an operator reads.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Anomaly => "anomaly",
            Self::Policy => "policy",
        }
    }
}

impl Gates {
    /// Stop `fleet_id`, and drop its readiness mark.
    ///
    /// # Errors
    /// Reports a datastore that would not answer the pause. The caller absorbs
    /// it — the EVENT is killed by policy whether or not the fleet's own row
    /// could be flipped — but absorbing it is the caller's visible decision
    /// rather than something swallowed here.
    ///
    /// The readiness clear is best-effort and does NOT reach the caller: a
    /// stale mark costs one wasted candidate check on a later poll, and the
    /// fleet is already paused where it counts.
    pub async fn pause(&self, fleet_id: &Uuid7, trigger: Trigger, now: UnixMillis) -> Result<()> {
        let mut connection = self.database().acquire().await?;
        sqlx::query(sql::PAUSE_FLEET)
            .bind(now.as_millis())
            .bind(fleet_id.as_str())
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_PAUSE))?;

        let fleet = fleet_id.as_str();
        tracing::warn!(
            event = EVENT_FLEET_PAUSED,
            fleet_id = fleet,
            trigger = trigger.as_str(),
            "the fleet is stopped; no further work of its is admitted"
        );

        // Only after the pause committed. See the module note.
        if let Err(fault) = self.ready().force_clear(fleet).await {
            let reason = fault.to_string();
            tracing::warn!(
                event = EVENT_READY_STALE,
                fleet_id = fleet,
                reason,
                "a paused fleet kept its readiness mark; a later poll pays one wasted check"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Trigger;

    #[test]
    fn the_two_triggers_name_themselves_distinctly() {
        // They land in the same log field, so a shared or empty spelling would
        // make the two incidents indistinguishable to the operator the field
        // exists for.
        assert_ne!(Trigger::Anomaly.as_str(), Trigger::Policy.as_str());
        for trigger in [Trigger::Anomaly, Trigger::Policy] {
            assert!(!trigger.as_str().is_empty(), "{trigger:?}");
        }
    }
}
