//! Noticing that a runner stopped.
//!
//! A runner proves it is alive by beating. Nothing tells this daemon when one
//! dies — a host that lost power, a container the scheduler took, a process
//! that hung — so the only evidence is a beat that did not arrive, and someone
//! has to look. That is this sweeper.
//!
//! # Three things happen to a runner that went quiet
//!
//! It is recorded offline, ONCE per stale episode; the affinity slots it still
//! holds are released, so the fleets it was running become leasable again; and
//! if an operator asked it to drain, the drain is finished now that its last
//! lease is gone. The three are separate statements because they are separate
//! facts, and a runner can need any subset of them.
//!
//! # A never-seen runner is `registered`, not offline
//!
//! [`LAST_SEEN_NEVER`](crate::sql::LAST_SEEN_NEVER) is the sentinel a fresh
//! enrolment carries, and the fetch excludes it explicitly. Arithmetic alone
//! would make a runner enrolled a second ago look decades stale — `now - 0` is
//! enormous — and it would be reported offline before it ever had a chance to
//! beat. A runner that has never connected is not one that stopped: it is one
//! that has not started, which is a different word to an operator and a
//! different row to a dashboard (Dimension 6.3).

use std::time::Duration;

use afd_core::clock::{self, UnixMillis};
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_core::spelling;
use afd_core::timing::{HEARTBEAT_INTERVAL_MS, RUNNER_OFFLINE_AFTER_MS};
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_wire::admin::AdminState;
use sqlx::{Acquire as _, Row as _};

use crate::error::{Result, query};

use crate::sql;
use crate::sweep::{Sweep, Swept};

/// Statement name, for the context a query failure carries.
const CONTEXT_DUE: &str = "liveness due runners";

/// Statement name, for the context a query failure carries.
const CONTEXT_OFFLINE: &str = "liveness offline event";

/// Statement name, for the context a query failure carries.
const CONTEXT_SLOTS: &str = "liveness slot expiry";

/// Statement name, for the context a query failure carries.
const CONTEXT_DRAINED: &str = "liveness drain completion";

/// The event an unreadable `admin_state` is reported under.
const EVENT_UNMODELLED_STATE: &str = "runner_admin_state_unmodelled";

/// How many runners one pass considers.
///
/// A bound on the pass, not on the fleet: a deployment with a thousand stale
/// runners sweeps a hundred of them now and the rest on the next tick, in
/// least-recently-touched order, so none is starved. An unbounded pass would
/// hold a connection for as long as the trouble lasted.
const BATCH_LIMIT: i64 = 100;

/// How far into the past an expiring slot is stamped.
///
/// One millisecond, so the slot reads as already expired to every comparison
/// rather than as expiring exactly now — a `>` and a `>=` elsewhere would
/// otherwise disagree about a slot stamped at the current instant.
const EXPIRE_PAST_DELTA_MS: i64 = 1;

/// One runner the pass has to look at.
#[derive(Debug)]
struct Due {
    /// Which runner.
    id: Uuid7,
    /// When it last beat, or [`sql::LAST_SEEN_NEVER`].
    last_seen_at: i64,
    /// What an operator has asked of it.
    ///
    /// `None` is a column holding a word this daemon does not model, which is a
    /// data-integrity fault rather than a state to guess at — see
    /// [`Liveness::visit`] for what the pass does about it.
    admin_state: Option<AdminState>,
}

impl Due {
    /// Whether this runner has gone quiet for longer than it may.
    ///
    /// The sentinel is excluded HERE as well as in the statement, and the
    /// duplication is deliberate: this is the predicate a reader checks
    /// Dimension 6.3 against, and a fetch widened later must not silently make
    /// a never-seen runner offline.
    const fn is_stale(&self, now: UnixMillis) -> bool {
        self.last_seen_at != sql::LAST_SEEN_NEVER
            && now.as_millis().saturating_sub(self.last_seen_at) > RUNNER_OFFLINE_AFTER_MS
    }
}

/// The liveness pass, over the api-role pool.
#[derive(Debug, Clone)]
pub struct Liveness {
    /// Where the rows are.
    database: Db,
    /// The identifiers the event rows are minted from.
    entropy: Entropy,
}

impl Liveness {
    /// A sweeper reading and writing through `database`.
    #[must_use]
    pub const fn new(database: Db, entropy: Entropy) -> Self {
        Self { database, entropy }
    }

    /// A fresh identifier for one event row.
    fn event_id(&self, now: UnixMillis) -> Result<Uuid7> {
        let mut bytes = [0u8; ENTROPY_LEN];
        self.entropy.fill(&mut bytes)?;
        Ok(Uuid7::encode(now, bytes)?)
    }

    /// The runners this pass has to look at.
    async fn due(&self, now: UnixMillis) -> Result<Vec<Due>> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::sweep::SELECT_DUE_RUNNERS)
            .bind(sql::LAST_SEEN_NEVER)
            .bind(now.as_millis())
            .bind(RUNNER_OFFLINE_AFTER_MS)
            .bind(sql::ADMIN_STATE_ACTIVE)
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(sql::ADMIN_STATE_DRAINING)
            .bind(BATCH_LIMIT)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_DUE))?;

        rows.iter()
            .map(|row| {
                let id: String = row.try_get(0).map_err(query(CONTEXT_DUE))?;
                let raw: String = row.try_get(2).map_err(query(CONTEXT_DUE))?;
                Ok(Due {
                    id: Uuid7::parse(&id)?,
                    last_seen_at: row.try_get(1).map_err(query(CONTEXT_DUE))?,
                    // A state this daemon does not model is a row it cannot
                    // reason about, so the pass reports it rather than guessing
                    // — guessing `active` would leave a revoked runner leasing.
                    admin_state: spelling::from_spelling(&raw),
                })
            })
            .collect()
    }

    /// Everything one due runner needs done, and nothing it does not.
    async fn visit(&self, runner: &Due, now: UnixMillis) -> Result<u64> {
        let mut changed = 0;
        if runner.is_stale(now) {
            changed += self.record_offline(runner, now).await?;
        }
        // A runner an operator has taken out of service holds no slots, whether
        // or not it is still beating: cordoned and revoked are as final as
        // dead, from the fleet's side.
        let Some(admin_state) = runner.admin_state else {
            // A state this daemon does not model. The runner is left exactly as
            // it is and the pass CARRIES ON, where `liveness_sweeper.zig`
            // returns `error.DbRowShape` and abandons the whole batch — one
            // unreadable row there stops liveness for every other runner in it.
            tracing::warn!(
                runner_id = runner.id.as_str(),
                event = EVENT_UNMODELLED_STATE,
                "a runner's admin_state holds a word this daemon does not model; skipping it"
            );
            return Ok(changed);
        };
        if admin_state != AdminState::Active {
            changed += self.release_slots(&runner.id, now).await?;
        }
        if admin_state == AdminState::Draining {
            changed += self.finish_drain(&runner.id, now).await?;
        }
        Ok(changed)
    }

    /// Records the runner offline and releases what it was holding.
    ///
    /// One transaction, because the two must not disagree: an event saying a
    /// runner went offline while its slots stay held would leave a fleet
    /// unrunnable with a history that says the cause was handled.
    ///
    /// The slots are released only when the event was NEWLY inserted — a
    /// re-observed stale runner has already had them released, and the guard is
    /// what keeps the pass from re-doing work every ten seconds for the whole
    /// time a host is down.
    async fn record_offline(&self, runner: &Due, now: UnixMillis) -> Result<u64> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_OFFLINE))?;

        let event_id = self.event_id(now)?;
        let inserted: i64 = sqlx::query(sql::sweep::INSERT_OFFLINE_EVENT)
            .bind(event_id.as_str())
            .bind(runner.id.as_str())
            .bind(sql::event_type::RUNNER_OFFLINE)
            .bind(now.as_millis())
            .bind(sql::meta::LAST_SEEN_AT)
            .bind(runner.last_seen_at)
            .fetch_one(&mut *transaction)
            .await
            .and_then(|row| row.try_get(0))
            .map_err(query(CONTEXT_OFFLINE))?;
        if inserted == 0 {
            return Ok(0);
        }

        let expired = expire_slots(&mut *transaction, &runner.id, now).await?;
        transaction.commit().await.map_err(query(CONTEXT_OFFLINE))?;
        Ok(u64::try_from(expired).unwrap_or(0).saturating_add(1))
    }

    /// Releases the slots a runner holds without recording anything.
    ///
    /// The path for a runner an operator took out of service, which is not an
    /// offline EPISODE and gets no event: the operator already knows, having
    /// done it.
    async fn release_slots(&self, runner_id: &Uuid7, now: UnixMillis) -> Result<u64> {
        let mut connection = self.database.acquire().await?;
        let expired = expire_slots(&mut *connection, runner_id, now).await?;
        Ok(u64::try_from(expired).unwrap_or(0))
    }

    /// Finishes a drain, if the runner's last lease is gone.
    async fn finish_drain(&self, runner_id: &Uuid7, now: UnixMillis) -> Result<u64> {
        let mut connection = self.database.acquire().await?;
        let event_id = self.event_id(now)?;
        let drained: i64 = sqlx::query(sql::sweep::MARK_DRAINED_IF_IDLE)
            .bind(runner_id.as_str())
            .bind(sql::ADMIN_STATE_DRAINED)
            .bind(now.as_millis())
            .bind(sql::ADMIN_STATE_DRAINING)
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(event_id.as_str())
            .bind(sql::event_type::RUNNER_DRAINED)
            .bind(sql::meta::FROM_ADMIN_STATE)
            .bind(sql::meta::TO_ADMIN_STATE)
            .fetch_one(&mut *connection)
            .await
            .and_then(|row| row.try_get(0))
            .map_err(query(CONTEXT_DRAINED))?;
        Ok(u64::try_from(drained).unwrap_or(0))
    }
}

/// Releases one runner's slots through whatever connection the caller holds.
///
/// Takes the executor rather than acquiring its own, which is what lets the
/// offline path run it INSIDE its transaction and the operator path run it
/// alone. Two copies of the statement is how the two would come to differ.
async fn expire_slots<'a, E>(executor: E, runner_id: &Uuid7, now: UnixMillis) -> Result<i64>
where
    E: sqlx::Executor<'a, Database = sqlx::Postgres>,
{
    sqlx::query(sql::sweep::EXPIRE_ACTIVE_LEASE_SLOTS)
        .bind(runner_id.as_str())
        .bind(sql::LEASE_STATUS_ACTIVE)
        .bind(now.as_millis().saturating_sub(EXPIRE_PAST_DELTA_MS))
        .bind(now.as_millis())
        .fetch_one(executor)
        .await
        .and_then(|row| row.try_get(0))
        .map_err(query(CONTEXT_SLOTS))
}

impl Sweep for Liveness {
    fn name(&self) -> &'static str {
        "liveness"
    }

    /// Paced to the heartbeat, not to the offline threshold.
    ///
    /// A runner is declared offline after three lease TTLs, and this looks
    /// every heartbeat interval — so the delay between a runner going quiet and
    /// the plane noticing is bounded by the threshold plus one tick, rather
    /// than by twice the threshold.
    fn interval(&self) -> Duration {
        Duration::from_millis(u64::try_from(HEARTBEAT_INTERVAL_MS).unwrap_or(10_000))
    }

    async fn sweep(&self) -> Result<Swept> {
        let now = clock::now();
        let due = self.due(now).await?;
        let mut swept = Swept {
            scanned: u64::try_from(due.len()).unwrap_or(u64::MAX),
            changed: 0,
        };
        // Sequential, not concurrent: these are writes against rows a live
        // request path also touches, and a hundred of them at once would be a
        // hundred pooled connections spent on work nobody is waiting for.
        for runner in &due {
            swept.changed += self.visit(runner, now).await?;
        }
        Ok(swept)
    }
}

#[cfg(test)]
mod tests;
