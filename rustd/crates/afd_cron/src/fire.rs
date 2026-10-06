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

use afd_admission::{Admission, Admissions, Key, Producer, Reply};
use afd_core::id::Uuid7;
use afd_wire::event::EventType;

use crate::error::Result;
use crate::store::FireTarget;

/// What every schedule-driven wake's actor begins with.
///
/// The actor names the SCHEDULE and no person. A schedule was created by
/// somebody, but the fire was not — recording its author would let an
/// actor-shaped assertion certify that a human woke this fleet at 3am when a
/// cron did. Naming the schedule rather than the scheduler is what lets a
/// fleet list one schedule's runs, and it is the `cron:*` the dashboard
/// already filters cron runs by.
pub const ACTOR_PREFIX: &str = "cron:";

/// The actor a fire of `schedule` records: [`ACTOR_PREFIX`], then its id.
///
/// The one place the two are joined, so the fire that writes the actor and the
/// listing that reads a schedule's runs by it cannot spell it differently.
#[must_use]
pub fn schedule_actor(schedule: &Uuid7) -> String {
    format!("{ACTOR_PREFIX}{}", schedule.as_str())
}

/// The fire identity of a `once` schedule, in place of the caller's id.
///
/// A one-off fires once, whichever path asks: the scheduler's callback and a
/// run-now carry different ids, and both can read the row before either
/// retires it. Keyed by the schedule alone, the second is the first's replay.
const ONCE_FIRE: &str = "once";

/// The body field a fired run's words travel in.
///
/// The field every producer's body uses and the runner reads its first turn
/// from (`afr_agent::prompt`); see [`body`].
const FIELD_MESSAGE: &str = "message";

/// The event body a fire of a schedule whose message is `message` stores.
///
/// The schedule's message is the author's plain text, and the lease records
/// every event's body into a `jsonb` column (`afd_events::sql`). Stored as it
/// was, a message that is not itself JSON failed that cast, so the fired run
/// was admitted and could never be leased. Wrapped as `{"message": …}`, the
/// shape a steer and a mention store, the runner reads the schedule's words as
/// the run's first turn.
pub(crate) fn body(message: &str) -> String {
    let mut fields = serde_json::Map::new();
    fields.insert(FIELD_MESSAGE.to_owned(), message.into());
    serde_json::Value::Object(fields).to_string()
}

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

    /// Admits one verified fire, at most once however often it arrives. A
    /// `once` target is admitted at most once at all, under [`ONCE_FIRE`].
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
        let fire_id = if target.once { ONCE_FIRE } else { message_id };
        let key = format!("{fleet}:{schedule}:{fire_id}");
        let actor = schedule_actor(schedule);
        let request_json = body(&target.message);

        let admitted = self
            .admissions
            .admit(Admission {
                producer: Producer::ScheduleFire,
                key: Key::Repeated(&key),
                fleet,
                workspace: target.workspace.as_str(),
                actor: &actor,
                event_type: EventType::Cron,
                request_json: &request_json,
                // A schedule has no one to answer.
                reply: Reply::None,
            })
            .await?;

        // Hoisted rather than spelled inside the macro: the log bridge
        // duplicates every field expression and coverage instrumentation scores
        // the dead copy (`docs/LOGGING_STANDARD.md` §8A).
        let event_id = admitted.stored.id.as_str();
        let replayed = admitted.replayed;
        let schedule_id = schedule.as_str();
        let workspace_id = target.workspace.as_str();
        tracing::info!(
            fleet_id = fleet,
            workspace_id,
            schedule_id,
            event_id,
            replayed,
            event = "schedule_fire_appended",
        );

        Ok(Fired {
            event_id: admitted.stored.id,
            replayed,
        })
    }
}

#[cfg(test)]
mod tests;
