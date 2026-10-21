//! Stopping a fleet, when the gate decides no further work of its may run.
//!
//! # Two auto-kill paths, one stop
//!
//! A tripped anomaly rule and an `auto_kill` gate rule reach the same place: the
//! fleet stops, and the event that triggered it ends. They differ only in what
//! an operator is told, which is why the trigger travels as a value rather than
//! as two copies of this function.
//!
use afd_core::id::Uuid7;
use afd_fleet_lifecycle::FleetStatus;

use super::sql;
use crate::error::{Result, query};
use crate::gate::store::Gates;

/// Statement name, for the context a query failure carries.
const CONTEXT_PAUSE: &str = "fleet pause";

/// A fleet was stopped by the gate.
const EVENT_FLEET_PAUSED: &str = "gate_fleet_paused";

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
    /// Pause `fleet_id` so candidate discovery excludes it.
    ///
    /// # Errors
    /// Reports a datastore that would not answer the pause. The caller absorbs
    /// it — the EVENT is killed by policy whether or not the fleet's own row
    /// could be flipped — but absorbing it is the caller's visible decision
    /// rather than something swallowed here.
    ///
    /// The row is stamped on the database clock.
    pub async fn pause(&self, fleet_id: &Uuid7, trigger: Trigger) -> Result<()> {
        let mut connection = self.database().acquire().await?;
        sqlx::query(sql::PAUSE_FLEET)
            .bind(FleetStatus::Paused.as_str())
            .bind(fleet_id.as_str())
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_PAUSE))?;

        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let fleet = fleet_id.as_str();
        let trigger_word = trigger.as_str();
        tracing::warn!(
            event = EVENT_FLEET_PAUSED,
            fleet_id = fleet,
            trigger = trigger_word,
            "the fleet is stopped; no further work of its is admitted"
        );

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
