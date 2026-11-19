//! A fleet's steer: work one fleet puts on another fleet in its workspace.
//!
//! It is a person's steer in every field but four. The actor names the
//! sending fleet, and so does the body. The reply destination is copied from
//! the event the sender is running, so the target answers where the sender
//! was asked. The key is derived here, never named by a caller.
//!
//! It runs behind the lease's write guard, inside the admission's own
//! transaction, so a runner whose lease a reclaim took writes nothing. It
//! computes no disposition: a fleet's steer never folds into a live run, and
//! its sender does not wait to learn where it went.

use af_core::id::Uuid7;
use afd_admission::Unheld;
use afd_db::Precondition;

use super::admission::{fleet_operation_id, fleet_steer_admission, scoped_key};
use super::{Steer, answer_repeat};
use crate::error::Result;

/// The prefix every fleet-sent steer carries in its actor.
///
/// Never [`super::ACTOR_PREFIX`]: the readers that mean a person match
/// `steer:%`, and a fleet's steer is not a person's.
pub const FLEET_ACTOR_PREFIX: &str = "fleet:";

/// The actor a fleet's steer records: [`FLEET_ACTOR_PREFIX`], then the
/// sending fleet's id.
#[must_use]
pub fn fleet_actor(fleet: &Uuid7) -> String {
    format!("{FLEET_ACTOR_PREFIX}{}", fleet.as_str())
}

/// Whether `actor` is a fleet-sent steer's.
#[must_use]
pub fn is_fleet_actor(actor: &str) -> bool {
    actor.starts_with(FLEET_ACTOR_PREFIX)
}

/// One fleet's steer, as the runner plane hands it over.
#[derive(Debug, Clone, Copy)]
pub struct FleetSteer<'a> {
    /// The fleet the work goes to.
    pub target: &'a Uuid7,
    /// The workspace both fleets are in: the lease's, never the request's.
    pub workspace: &'a Uuid7,
    /// The fleet sending it.
    pub sender_fleet: &'a Uuid7,
    /// The event the sender is running: where the reply destination is
    /// copied from, and half the key.
    pub sender_event: &'a Uuid7,
    /// The sender's words, with its secret values masked.
    pub masked_text: &'a str,
    /// The stored body, `af_wire::event::FleetSteerBody`, serialized.
    pub request_json: &'a str,
}

/// What a fleet's steer was answered with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetSteered {
    /// The event the steer became on the target.
    pub event_id: String,
    /// When THIS call admitted it, or `None` when an earlier send of the same
    /// words from the same event already had.
    pub admitted_at: Option<jiff::Timestamp>,
}

impl FleetSteered {
    /// Whether an earlier send of the same words already admitted it.
    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.admitted_at.is_none()
    }

    /// The answer to a repeat: the first send's event.
    const fn repeat(event_id: String) -> Self {
        Self {
            event_id,
            admitted_at: None,
        }
    }
}

impl Steer {
    /// Puts one fleet's message on another fleet's stream, behind `guard`.
    ///
    /// # Errors
    /// Refuses a target this workspace does not hold, one that takes no new
    /// work, a setup needing review, a spent deployment ceiling, and the same
    /// key already holding other words. A guard that does not hold is not an
    /// error: it answers `Ok(Err(Unheld))` with nothing written.
    pub async fn append_from_fleet(
        &self,
        steer: FleetSteer<'_>,
        guard: &dyn Precondition,
    ) -> Result<core::result::Result<FleetSteered, Unheld>> {
        let actor = fleet_actor(steer.sender_fleet);
        let operation = fleet_operation_id(steer.sender_event, steer.masked_text);
        let key = scoped_key(steer.target.as_str(), &operation);
        let admission = fleet_steer_admission(
            steer.target,
            steer.workspace,
            &actor,
            steer.request_json,
            &key,
            steer.sender_event,
        );
        let admitted = match self
            .admissions
            .admit_runnable_guarded(admission, guard)
            .await
        {
            Ok(Ok(admitted)) => admitted,
            Ok(Err(unheld)) => return Ok(Err(unheld)),
            Err(refused) if refused.is_over_capacity() => {
                return self
                    .repeat_despite(&admission, refused)
                    .await
                    .map(|event_id| Ok(FleetSteered::repeat(event_id)));
            }
            Err(failed) => return Err(failed.into()),
        };
        if admitted.replayed {
            return answer_repeat(&admission, admitted.stored)
                .map(|event_id| Ok(FleetSteered::repeat(event_id)));
        }
        Ok(Ok(FleetSteered {
            event_id: admitted.stored.id.into(),
            admitted_at: Some(admitted.stored.created_at),
        }))
    }
}

#[cfg(test)]
mod tests {
    use af_core::id::Uuid7;

    use super::super::admission::fleet_operation_id;
    use super::{FLEET_ACTOR_PREFIX, fleet_actor, is_fleet_actor};
    use crate::steer::{ACTOR_MACHINE, is_steer_actor, steer_actor};

    const FLEET: &str = "0190d5c2-7e1a-7000-8000-00000000f001";
    const EVENT: &str = "0190d5c2-7e1a-7000-8000-00000000e001";

    fn id(text: &str) -> Uuid7 {
        Uuid7::parse(text).unwrap_or_else(|_| unreachable!("a fixture id parses"))
    }

    /// A fleet's actor is never a person's, and a person's is never a fleet's.
    #[test]
    fn a_fleet_actor_is_never_a_steer_actor() {
        let actor = fleet_actor(&id(FLEET));
        assert_eq!(actor, format!("{FLEET_ACTOR_PREFIX}{FLEET}"));
        assert!(is_fleet_actor(&actor));
        assert!(!is_steer_actor(&actor));
        assert!(!is_fleet_actor(&steer_actor("user_2abc")));
        assert!(!is_fleet_actor(ACTOR_MACHINE));
        assert!(!is_fleet_actor("fleet"));
    }

    /// The same words from the same event are one operation; other words, or
    /// the same words from another event, are another.
    #[test]
    fn the_operation_id_is_the_event_and_a_digest_of_the_words() {
        let event = id(EVENT);
        let first = fleet_operation_id(&event, "run the hourly check");
        assert_eq!(first, fleet_operation_id(&event, "run the hourly check"));
        assert_ne!(first, fleet_operation_id(&event, "run it again"));
        let other = id("0190d5c2-7e1a-7000-8000-00000000e002");
        assert_ne!(first, fleet_operation_id(&other, "run the hourly check"));
        let (head, digest) = first.split_once(':').unwrap_or_default();
        assert_eq!(head, EVENT);
        assert_eq!(digest.len(), 32);
        assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
