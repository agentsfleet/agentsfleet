//! A verified schedule fire, on the stream exactly once.
//!
//! # The claim key is the scheduler's message id, and it has to be
//!
//! The external scheduler retries a callback it did not get a 2xx for, and it
//! repeats its own message id when it does. That id is therefore the only value
//! that identifies "this fire" across attempts — a key minted here would make
//! every retry a new fire, which is the duplicate run this exists to prevent,
//! and the body's digest would collapse two genuinely separate fires of the
//! same schedule into one.
//!
//! Unlike `x-github-delivery`, the id is not an unauthenticated header: it is
//! the `jti` claim INSIDE the signed token ([`crate::verifier`]), so a captured
//! callback resent under a fresh id no longer verifies.
//!
//! # Concurrency is the point, not an edge case
//!
//! Two daemons behind one load balancer can both receive the same retry at
//! the same moment. Both commit the same ledger row — one inserts and one
//! conflicts, serialised by the row lock the conflict arm takes — so exactly
//! one of them owns the append. There is no window between "check" and
//! "write" for them to both pass through, and unlike the Lua claim this
//! replaces, the row does not expire and does not live in the queue.

use afd_admission::{Admission, Admissions, Key, Producer};
use afd_core::id::Uuid7;
use afd_wire::event::EventType;

use crate::error::Result;
use crate::store::FireTarget;

/// What a schedule-driven wake records as the actor.
///
/// Names the SCHEDULER and no person. A schedule was created by somebody, but
/// the fire was not — recording its author would let an actor-shaped assertion
/// certify that a human woke this fleet at 3am when a cron did.
const ACTOR_SCHEDULE: &str = "schedule:qstash";

/// What one fire put on the stream.
#[derive(Debug, Clone)]
pub struct Fired {
    /// The event's id — this fire's, or the attempt that beat it.
    pub event_id: String,
    /// Whether an earlier attempt already wrote it.
    pub replayed: bool,
}

/// The ledger a verified fire is admitted through.
///
/// Cheap to clone: [`Admissions`] is a pair of handles over shared pools.
#[derive(Debug, Clone)]
pub struct Fire {
    /// Where the fire is accepted, before anything is queued.
    admissions: Admissions,
}

impl Fire {
    /// Binds the appender to an already-connected ledger.
    #[must_use]
    pub const fn new(admissions: Admissions) -> Self {
        Self { admissions }
    }

    /// Admits one verified fire, at most once however often it arrives.
    ///
    /// # Errors
    /// Reports a database that would not record the acceptance. A queue that
    /// would not take the entry is NOT an error — the fire is durable and the
    /// replay sweeper delivers it.
    pub async fn deliver(
        &self,
        schedule: &Uuid7,
        target: &FireTarget,
        message_id: &str,
    ) -> Result<Fired> {
        let fleet = target.fleet.as_str();
        // Scoped by SCHEDULE as well as by fleet: one fleet may hold many
        // schedules, and a key that was the message id alone would let two
        // schedules firing on the same tick silence each other.
        let key = format!("{fleet}:{schedule}:{message_id}");

        let admitted = self
            .admissions
            .admit(Admission {
                producer: Producer::ScheduleFire,
                key: Key::Repeated(&key),
                fleet,
                workspace: target.workspace.as_str(),
                actor: ACTOR_SCHEDULE,
                event_type: EventType::Cron,
                request_json: &target.message,
            })
            .await?;

        // Hoisted rather than spelled inside the macro: the log bridge
        // duplicates every field expression and coverage instrumentation scores
        // the dead copy (`docs/LOGGING_STANDARD.md` §8A).
        let event_id = admitted.id.as_str();
        let replayed = admitted.replayed;
        let schedule_id = schedule.as_str();
        tracing::info!(
            fleet_id = fleet,
            workspace_id = target.workspace.as_str(),
            schedule_id,
            event_id,
            replayed,
            event = "schedule_fire_appended",
        );

        Ok(Fired {
            event_id: admitted.id,
            replayed,
        })
    }
}
