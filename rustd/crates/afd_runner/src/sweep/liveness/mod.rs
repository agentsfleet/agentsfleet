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
//! [`NEVER_SEEN`](crate::sql::NEVER_SEEN) is the sentinel a fresh enrolment
//! carries, and the fetch excludes it explicitly. Arithmetic alone would make a
//! runner enrolled a second ago look decades stale — the sentinel is the epoch
//! — and it would be reported offline before it ever had a chance to beat. A runner that has never connected is not one that stopped: it is one
//! that has not started, which is a different word to an operator and a
//! different row to a dashboard (Dimension 6.3).

use std::time::Duration;

use afd_api_wire::admin::AdminState;
use afd_core::clock;
use afd_core::id::Uuid7;
use afd_core::spelling;
use afd_core::timing::{HEARTBEAT_INTERVAL, RUNNER_OFFLINE_AFTER};
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::Acquire as _;

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

/// The event an unreadable `status` is reported under.
const EVENT_UNMODELLED_STATE: &str = "runner_status_unmodelled";

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
const EXPIRE_PAST: Duration = Duration::from_millis(1);

/// One runner the pass has to look at.
#[derive(Debug)]
struct Due {
    /// Which runner.
    id: Uuid7,
    /// When it last beat, or [`sql::NEVER_SEEN`].
    last_seen_at: Timestamp,
    /// The database's clock when the fetch selected it.
    checked_at: Timestamp,
    /// What an operator has asked of it.
    ///
    /// `None` is a column holding a word this daemon does not model, which is a
    /// data-integrity fault rather than a state to guess at — see
    /// [`Liveness::visit`] for what the pass does about it.
    status: Option<AdminState>,
}

impl Due {
    /// Whether this runner had gone quiet for longer than it may when the
    /// fetch read it.
    ///
    /// The sentinel is excluded HERE as well as in the statement, and the
    /// duplication is deliberate: this is the predicate a reader checks
    /// Dimension 6.3 against, and a fetch widened later must not silently make
    /// a never-seen runner offline. Judged at the database's clock, the same
    /// instant the fetch's own staleness test used.
    fn is_stale(&self) -> bool {
        self.last_seen_at != sql::NEVER_SEEN
            && self.last_seen_at < clock::before(self.checked_at, RUNNER_OFFLINE_AFTER)
    }
}

/// [`sql::sweep::SELECT_DUE_RUNNERS`]'s row, by column name.
#[derive(Debug, sqlx::FromRow)]
struct DueRow {
    id: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    last_seen_at: Timestamp,
    status: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    checked_at: Timestamp,
}

impl DueRow {
    /// The runner this row names.
    ///
    /// A status this daemon does not model is a row it cannot reason about,
    /// so the pass reports it rather than guessing — guessing `active` would
    /// leave a revoked runner leasing.
    fn parse(self) -> Result<Due> {
        Ok(Due {
            id: Uuid7::parse(&self.id)?,
            last_seen_at: self.last_seen_at,
            checked_at: self.checked_at,
            status: spelling::from_spelling(&self.status),
        })
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
    fn event_id(&self) -> Result<Uuid7> {
        Ok(self.entropy.uuid7(clock::now())?)
    }

    /// The runners this pass has to look at.
    async fn due(&self) -> Result<Vec<Due>> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query_as::<_, DueRow>(sql::sweep::SELECT_DUE_RUNNERS)
            .bind(sql::NEVER_SEEN.to_sqlx())
            .bind(RUNNER_OFFLINE_AFTER)
            .bind(sql::ADMIN_STATE_ACTIVE)
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(sql::ADMIN_STATE_DRAINING)
            .bind(BATCH_LIMIT)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_DUE))?;
        rows.into_iter().map(DueRow::parse).collect()
    }

    /// Everything one due runner needs done, and nothing it does not.
    async fn visit(&self, runner: &Due) -> Result<u64> {
        let mut changed = 0;
        if runner.is_stale() {
            changed += self.record_offline(runner).await?;
        }
        // A runner an operator has taken out of service holds no slots, whether
        // or not it is still beating: cordoned and revoked are as final as
        // dead, from the fleet's side.
        let Some(status) = runner.status else {
            // A state this daemon does not model. The runner is left exactly as
            // it is and the pass CARRIES ON: abandoning the batch would let one
            // unreadable row stop liveness for every other runner in it.
            tracing::warn!(
                runner_id = runner.id.as_str(),
                event = EVENT_UNMODELLED_STATE,
                "a runner's status holds a word this daemon does not model; skipping it"
            );
            return Ok(changed);
        };
        if status != AdminState::Active {
            changed += self.release_slots(&runner.id).await?;
        }
        if status == AdminState::Draining {
            changed += self.finish_drain(&runner.id).await?;
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
    async fn record_offline(&self, runner: &Due) -> Result<u64> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_OFFLINE))?;

        let event_id = self.event_id()?;
        let inserted: i64 = sqlx::query_scalar(sql::sweep::INSERT_OFFLINE_EVENT)
            .bind(event_id.as_str())
            .bind(runner.id.as_str())
            .bind(sql::event_type::RUNNER_OFFLINE)
            .bind(sql::meta::LAST_SEEN_AT)
            .bind(runner.last_seen_at.to_sqlx())
            .fetch_one(&mut *transaction)
            .await
            .map_err(query(CONTEXT_OFFLINE))?;
        if inserted == 0 {
            return Ok(0);
        }

        let expired = expire_slots(&mut *transaction, &runner.id).await?;
        transaction.commit().await.map_err(query(CONTEXT_OFFLINE))?;
        Ok(u64::try_from(expired).unwrap_or(0).saturating_add(1))
    }

    /// Releases the slots a runner holds without recording anything.
    ///
    /// The path for a runner an operator took out of service, which is not an
    /// offline EPISODE and gets no event: the operator already knows, having
    /// done it.
    async fn release_slots(&self, runner_id: &Uuid7) -> Result<u64> {
        let mut connection = self.database.acquire().await?;
        let expired = expire_slots(&mut *connection, runner_id).await?;
        Ok(u64::try_from(expired).unwrap_or(0))
    }

    /// Finishes a drain, if the runner's last lease is gone.
    async fn finish_drain(&self, runner_id: &Uuid7) -> Result<u64> {
        let mut connection = self.database.acquire().await?;
        let event_id = self.event_id()?;
        let drained: i64 = sqlx::query_scalar(sql::sweep::MARK_DRAINED_IF_IDLE)
            .bind(runner_id.as_str())
            .bind(sql::ADMIN_STATE_DRAINED)
            .bind(sql::ADMIN_STATE_DRAINING)
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(event_id.as_str())
            .bind(sql::event_type::RUNNER_DRAINED)
            .bind(sql::meta::FROM_STATUS)
            .bind(sql::meta::TO_STATUS)
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT_DRAINED))?;
        Ok(u64::try_from(drained).unwrap_or(0))
    }
}

/// Releases one runner's slots through whatever connection the caller holds.
///
/// Takes the executor rather than acquiring its own, which is what lets the
/// offline path run it INSIDE its transaction and the operator path run it
/// alone. Two copies of the statement is how the two would come to differ.
async fn expire_slots<'a, E>(executor: E, runner_id: &Uuid7) -> Result<i64>
where
    E: sqlx::Executor<'a, Database = sqlx::Postgres>,
{
    sqlx::query_scalar(sql::sweep::EXPIRE_ACTIVE_LEASE_SLOTS)
        .bind(runner_id.as_str())
        .bind(sql::LEASE_STATUS_ACTIVE)
        .bind(EXPIRE_PAST)
        .fetch_one(executor)
        .await
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
        HEARTBEAT_INTERVAL
    }

    async fn sweep(&self) -> Result<Swept> {
        let due = self.due().await?;
        let mut swept = Swept {
            scanned: u64::try_from(due.len()).unwrap_or(u64::MAX),
            changed: 0,
        };
        // Sequential, not concurrent: these are writes against rows a live
        // request path also touches, and a hundred of them at once would be a
        // hundred pooled connections spent on work nobody is waiting for.
        for runner in &due {
            swept.changed += self.visit(runner).await?;
        }
        Ok(swept)
    }
}

#[cfg(test)]
mod tests;
