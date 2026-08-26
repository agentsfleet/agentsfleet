//! The reads a recorded gate is resolved through: two Redis keys and one row.
//!
//! Every key shape this module writes or reads is declared in [`key`], once.
//! The sweeper, the resolver and the webhook handler have to agree on the exact
//! bytes or a pending gate becomes unreachable, and a prefix spelled inline at
//! one call site is precisely how that agreement ends (RULE UFS).

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_redis::{ReadyIndex, Redis};
use sqlx::Row as _;

use crate::error::{Result, query};
use crate::gate::decision::{Answer, Status};
use crate::gate::pending::{Evaluation, GateRef, evaluate};
use crate::sql;

/// Statement name, for the context a query failure carries.
const CONTEXT_STATUS: &str = "approval gate status";

/// The durable fallback answered where the mirror did not.
const EVENT_DB_FALLBACK: &str = "approval_decision_db_fallback_used";

/// How long a reference outlives its own deadline, in seconds.
///
/// A re-encountered event has to be able to resolve its gate AFTER the deadline
/// passes — that is what produces the timeout — so the key cannot expire at the
/// deadline. The grace covers the sweeper's cadence plus clock slop between
/// this daemon and Redis.
const REF_GRACE_SECONDS: i64 = 600;

/// The floor a reference's lifetime is clamped to, in seconds.
///
/// A gate raised with a deadline already in the past would otherwise compute a
/// negative expiry and be rejected by Redis. Two hours is the Zig's
/// `GATE_PENDING_TTL_SECONDS`.
const REF_MINIMUM_SECONDS: i64 = 7_200;

/// Milliseconds in a second, for the deadline-to-expiry conversion.
const MILLIS_PER_SECOND: i64 = 1_000;

/// Every Redis key the approval gate uses.
///
/// Prefixes rather than formatted keys, because two of the three are also read
/// by the sweeper and the webhook handler — a shape declared here and rebuilt
/// there is a shape that can drift by one colon.
pub mod key {
    /// `{fleet}:{event}` → the reference a parked event resolves through.
    const EVENT_REF_PREFIX: &str = "fleet:gate:byevent:";

    /// `{action}` → the mirrored decision.
    const RESPONSE_PREFIX: &str = "fleet:gate:response:";

    /// `{fleet}:{tool}:{action}` → the anomaly window's counter.
    const ANOMALY_PREFIX: &str = "fleet:anomaly:";

    /// The key holding `event_id`'s gate reference.
    #[must_use]
    pub fn event_ref(fleet_id: &str, event_id: &str) -> String {
        format!("{EVENT_REF_PREFIX}{fleet_id}:{event_id}")
    }

    /// The key holding `action_id`'s mirrored decision.
    #[must_use]
    pub fn response(action_id: &str) -> String {
        format!("{RESPONSE_PREFIX}{action_id}")
    }

    /// The key counting one `(fleet, tool, action)` inside its window.
    #[must_use]
    pub fn anomaly(fleet_id: &str, tool: &str, action: &str) -> String {
        format!("{ANOMALY_PREFIX}{fleet_id}:{tool}:{action}")
    }
}

/// The gate reads, over both datastores.
///
/// Both, because a gate genuinely spans them: the question and its answer are
/// durable in Postgres, and the hot-path mirror plus the event reference are in
/// Redis. A store holding one of them would leave the fallback — the whole
/// reason the durable read exists — impossible to express.
#[derive(Debug, Clone)]
pub struct Gates {
    pub(super) database: Db,
    queue: Redis,
    entropy: Entropy,
}

impl Gates {
    /// Gate reads and writes through `database` and `queue`.
    ///
    /// The entropy source is the third, and it arrives for one reason: raising
    /// a gate mints two identifiers — the action a human answers about and the
    /// row that records it — and they are drawn through the workspace's one
    /// entropy surface rather than a second source with its own failure mode.
    /// [`Leases`](crate::lease::Leases) takes it for the same reason.
    #[must_use]
    pub const fn new(database: Db, queue: Redis, entropy: Entropy) -> Self {
        Self {
            database,
            queue,
            entropy,
        }
    }

    /// The queue these gates are mirrored in, for the sibling module that
    /// counts anomalies through it.
    pub(super) const fn queue(&self) -> &Redis {
        &self.queue
    }

    /// The datastore the durable half of a gate lives in.
    pub(super) const fn database(&self) -> &Db {
        &self.database
    }

    /// The entropy source a raised gate draws its identifiers from.
    pub(super) const fn entropy(&self) -> &Entropy {
        &self.entropy
    }

    /// The readiness index a paused fleet is dropped from.
    ///
    /// Built per call rather than held: it is a thin binding over the same
    /// connection this store already owns, and storing a second handle to one
    /// connection would suggest there were two.
    pub(super) fn ready(&self) -> ReadyIndex {
        ReadyIndex::new(self.queue.clone())
    }

    /// The gate `event_id` is already waiting on, if any.
    ///
    /// A reference that does not parse answers `Ok(None)` — the same as one
    /// that was never written. Both mean this poll has no recorded gate to
    /// honour, and a reference nobody can read is better replaced by a fresh
    /// question than half-honoured.
    ///
    /// # Errors
    /// Reports a queue that would not answer. The caller must NOT collapse that
    /// into `None`: absent means never parked, and unreadable means we cannot
    /// tell — raising a second card for an event that may already hold one is
    /// worse than waiting a poll.
    pub async fn recorded(&self, fleet_id: &Uuid7, event_id: &str) -> Result<Option<GateRef>> {
        let stored = self
            .queue
            .get_string(&key::event_ref(fleet_id.as_str(), event_id))
            .await?;
        // `ok()` rather than `?`: a reference that will not deserialize means
        // this poll has no recorded gate to honour, which is the same answer as
        // one that was never written. See the doc note above.
        Ok(stored.and_then(|raw| serde_json::from_str(&raw).ok()))
    }

    /// Record that `event_id` is waiting on `reference`.
    ///
    /// # Errors
    /// Reports a queue that would not answer.
    pub async fn record(
        &self,
        fleet_id: &Uuid7,
        event_id: &str,
        reference: &GateRef,
    ) -> Result<()> {
        let lifetime = reference_lifetime(reference, afd_core::clock::now());
        // Infallible for this shape — a two-field record of a string and an
        // integer has no serializer error to reach — but the failure is not
        // swallowed: an unwritable reference would leave a parked event unable
        // to find its own gate, which is the one outcome worth an error.
        let encoded = serde_json::to_string(reference).map_err(|_shape| {
            crate::error::rejected(crate::error::DETAIL_GATE_REFERENCE_UNWRITABLE)
        })?;
        self.queue
            .set_for(
                &key::event_ref(fleet_id.as_str(), event_id),
                &encoded,
                lifetime,
            )
            .await
            .map_err(Into::into)
    }

    /// What this poll makes of `reference`.
    ///
    /// Reads the mirror first and the durable row only when the mirror is
    /// silent, which is one round trip on the path that runs and two on the
    /// path that should not. See the module note on [`super::pending`] for why
    /// the fallback exists at all.
    ///
    /// # Errors
    /// Reports a queue or datastore that would not answer.
    pub async fn evaluate(&self, reference: &GateRef, now: UnixMillis) -> Result<Evaluation> {
        Ok(evaluate(reference, self.answer(reference).await?, now))
    }

    /// The decision for `reference`, from whichever store has one.
    async fn answer(&self, reference: &GateRef) -> Result<Option<Answer>> {
        let action = reference.action_id().as_str();
        let mirrored = self.queue.get_string(&key::response(action)).await?;
        if let Some(answer) = mirrored.as_deref().and_then(Answer::parse) {
            return Ok(Some(answer));
        }

        let durable = self.status(action).await?.and_then(Status::answer);
        if let Some(answer) = durable {
            // Only reachable in the window a mirror write missed, which is what
            // makes it worth a line: this is the metric that says the write
            // side has a gap, and it is silent when it does not.
            tracing::info!(
                event = EVENT_DB_FALLBACK,
                action_id = action,
                outcome = answer.as_str(),
                "the decision mirror was absent and the durable row answered"
            );
        }
        Ok(durable)
    }

    /// The durable status of `action_id`, or nothing if it has no row.
    async fn status(&self, action_id: &str) -> Result<Option<Status>> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::gate::SELECT_GATE_STATUS)
            .bind(action_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_STATUS))?;

        row.map(|row| row.try_get::<String, _>(0).map_err(query(CONTEXT_STATUS)))
            .transpose()
            // A spelling this daemon does not know is treated as no status at
            // all, which leaves the gate PENDING. Guessing in either direction
            // would either release an event on an unrecognised word or kill one.
            .map(|stored| stored.as_deref().and_then(Status::parse))
    }
}

/// How long a reference should live, in seconds.
///
/// The deadline plus a grace, floored at the minimum — so a gate raised with a
/// deadline already behind it still gets a key that outlives the poll which
/// will resolve it, rather than a negative expiry Redis refuses.
fn reference_lifetime(reference: &GateRef, now: UnixMillis) -> i64 {
    let remaining = (reference.deadline().as_millis() - now.as_millis()) / MILLIS_PER_SECOND;
    remaining
        .saturating_add(REF_GRACE_SECONDS)
        .max(REF_MINIMUM_SECONDS)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{MILLIS_PER_SECOND, REF_MINIMUM_SECONDS, key, reference_lifetime};
    use crate::gate::pending::GateRef;
    use afd_core::clock::UnixMillis;
    use afd_core::id::Uuid7;

    const ACTION: &str = "0193e9a0-0000-7000-8000-00000000aaaa";

    /// The instant every case measures its deadline from.
    const NOW_MS: i64 = 0;

    /// A deadline comfortably beyond the grace, in seconds.
    const FAR_DEADLINE_S: i64 = 100_000;

    /// A deadline inside the floor, so the clamp is what answers.
    const NEAR_DEADLINE_MS: i64 = 1_000;

    /// An instant well past a deadline of zero, so the remaining span is
    /// negative and the floor is the only thing keeping the expiry valid.
    const LATE_NOW_MS: i64 = 10_000_000;

    fn reference(deadline_ms: i64) -> GateRef {
        GateRef::new(
            Uuid7::parse(ACTION).expect("a canonical identifier"),
            UnixMillis::from_millis(deadline_ms),
        )
    }

    #[test]
    fn every_key_is_built_from_its_declared_prefix() {
        // Pinned byte for byte: the sweeper, the resolver and the webhook
        // handler read these, and one changed colon makes a pending gate
        // unreachable rather than failing loudly.
        assert_eq!(
            key::event_ref("fleet-1", "event-9"),
            "fleet:gate:byevent:fleet-1:event-9"
        );
        assert_eq!(
            key::response(ACTION),
            format!("fleet:gate:response:{ACTION}")
        );
        assert_eq!(
            key::anomaly("fleet-1", "shell", "run"),
            "fleet:anomaly:fleet-1:shell:run"
        );
    }

    #[test]
    fn a_reference_outlives_its_own_deadline() {
        // The property the grace exists for: the poll that produces the TIMEOUT
        // runs after the deadline, so a key expiring at the deadline would take
        // the reference away before anything could resolve it.
        let now = UnixMillis::from_millis(NOW_MS);
        let far = reference(FAR_DEADLINE_S * MILLIS_PER_SECOND);

        let lifetime = reference_lifetime(&far, now);
        assert!(
            lifetime > FAR_DEADLINE_S,
            "a reference must outlive its deadline, got {lifetime}"
        );
    }

    #[test]
    fn a_deadline_already_past_still_gets_a_usable_lifetime() {
        // A negative expiry is one Redis refuses outright, which would leave
        // the event parked with no reference at all.
        let now = UnixMillis::from_millis(LATE_NOW_MS);
        let stale = reference(NOW_MS);

        assert_eq!(reference_lifetime(&stale, now), REF_MINIMUM_SECONDS);
    }

    #[test]
    fn a_near_deadline_is_floored_rather_than_shortened() {
        let now = UnixMillis::from_millis(NOW_MS);
        let soon = reference(NEAR_DEADLINE_MS);

        assert_eq!(reference_lifetime(&soon, now), REF_MINIMUM_SECONDS);
    }
}
