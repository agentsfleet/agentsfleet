//! The schedule table's reads and writes, and the fence that serialises them.
//!
//! # Every mutation leaves the row CLAIMED
//!
//! A create, an edit and a `:sync` all end with `sync_status = syncing`, a
//! token, and a lease. That is not bookkeeping: the caller is expected to go and
//! talk to the external scheduler next, and the claim is what stops a second
//! caller doing the same thing at the same time. The pair to every claim is a
//! finalize, which releases the fence and records whether the push worked.
//!
//! A caller that claims and then dies leaves a lease that expires. The next
//! claim takes the row because the fence predicate admits a lease in the past —
//! which is why the lease exists rather than a plain "held" flag, and why a
//! crashed syncer costs a lease window rather than a stuck schedule.
//!
//! # Absent is `Ok(None)`, not an error
//!
//! A schedule id that names no row, one belonging to another fleet, and one
//! another syncer is holding are all `Ok(None)`. None of them is this daemon
//! failing; each is a different answer the caller renders. Only the datastore
//! refusing, or a row this build cannot read, is an [`Error`].

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use sqlx::{Acquire as _, FromRow as _, Row as _};

use crate::error::{self, COLUMN_DESIRED_STATUS, COLUMN_FLEET, COLUMN_WORKSPACE, Result};
use crate::model::{DesiredStatus, Schedule, Source, SyncStatus};
use crate::sql;

mod admit;
mod decode;
mod fence;
pub(crate) mod refused;

pub use self::refused::Refused;

/// The columns [`Schedules::fire_target`] reads its answer from.
///
/// Named, not positional — see [`decode`] on why an index makes the statement's
/// column order load-bearing.
const COLUMN_MESSAGE: &str = "message";
/// See [`COLUMN_MESSAGE`].
const COLUMN_ONCE: &str = "once";
/// See [`COLUMN_MESSAGE`].
const COLUMN_FLEET_STATUS: &str = "status";

/// The context a failed read reports under.
const CONTEXT_READ: &str = "read a schedule";

/// The context a failed write reports under.
const CONTEXT_WRITE: &str = "write a schedule";

/// How long a syncer's hold on a row lasts before another may take it.
///
/// Long enough for an upstream round trip and its retries, short enough that a
/// syncer killed mid-push does not strand a schedule for a person watching the
/// list. `FireStore.zig` uses the same window.
pub const SYNC_LEASE_MS: i64 = 30_000;

/// What a schedule is created from.
///
/// A struct rather than eight positional arguments because four of them are
/// `&str` a call site could transpose without the compiler noticing.
#[derive(Debug, Clone, Copy)]
pub struct NewSchedule<'a> {
    /// The fleet the schedule wakes.
    pub fleet: &'a Uuid7,
    /// Who asked for it.
    pub source: Source,
    /// The key the external scheduler will know it by.
    pub source_key: &'a str,
    /// The expression, already through [`crate::validate::cron`].
    pub cron: &'a str,
    /// The zone, already through [`crate::validate::timezone`].
    pub timezone: &'a str,
    /// The message, already through [`crate::validate::message`].
    pub message: &'a str,
    /// Whether it retires after its first fire.
    pub once: bool,
}

/// What an edit changes, field by field.
///
/// Every field is optional and an absent one is left alone — the `COALESCE`
/// half of [`sql::CLAIM_MUTATION`]. A struct of `Option` rather than a full
/// replacement value, because a partial edit expressed as a whole row would
/// make every caller read before it writes and race its own read.
#[derive(Debug, Clone, Copy, Default)]
pub struct Change<'a> {
    /// A new expression, or the one already stored.
    pub cron: Option<&'a str>,
    /// A new zone, or the one already stored.
    pub timezone: Option<&'a str>,
    /// A new message, or the one already stored.
    pub message: Option<&'a str>,
    /// A new intent, or the one already stored.
    pub desired_status: Option<DesiredStatus>,
}

/// Where a fire is going, once a signed callback resolved to it.
#[derive(Debug, Clone)]
pub struct FireTarget {
    /// The fleet to wake.
    pub fleet: Uuid7,
    /// The workspace the event is stamped with.
    pub workspace: Uuid7,
    /// What the fleet is asked to do.
    pub message: String,
    /// Whether the schedule retires once this fire is admitted.
    pub once: bool,
    /// What the operator wants this schedule to be doing.
    pub desired_status: DesiredStatus,
    /// What the fleet's own row says it is doing.
    pub fleet_status: String,
}

/// The schedules table, over an already-connected pool.
///
/// Cheap to clone: [`Db`] is a handle over a shared pool.
#[derive(Debug, Clone)]
pub struct Schedules {
    /// Where every statement here runs.
    database: Db,
    /// Where a new schedule's identifier comes from.
    ///
    /// Held rather than taken per call for the reason `afd_admin::Models` holds
    /// one: the source is chosen by the binary, and a suite drives the mocked
    /// one so a created row's identifier is a fact a test can name.
    entropy: Entropy,
}

impl Schedules {
    /// Binds the store to an already-connected pool.
    #[must_use]
    pub const fn new(database: Db, entropy: Entropy) -> Self {
        Self { database, entropy }
    }

    /// A new schedule's identifier, ordered by the instant it was minted.
    ///
    /// `afd_admin::Models::mint`'s shape: the instant leads so rows sort by
    /// creation without a second column, and the entropy is what makes two
    /// minted in the same millisecond distinct.
    fn mint(&self, now: UnixMillis) -> Result<Uuid7> {
        Ok(self.entropy.uuid7(now)?)
    }

    /// This fleet's schedules, oldest first.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a row this build cannot
    /// read.
    pub async fn list(&self, fleet: &Uuid7) -> Result<Vec<Schedule>> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::LIST_FOR_FLEET)
            .bind(fleet.as_str())
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_READ))?;

        rows.iter()
            .map(Schedule::from_row)
            .collect::<sqlx::Result<Vec<_>>>()
            .map_err(error::query(CONTEXT_READ))
    }

    /// One schedule of this fleet's.
    ///
    /// # Errors
    /// As [`Self::list`].
    pub async fn one(&self, fleet: &Uuid7, schedule: &Uuid7) -> Result<Option<Schedule>> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::SELECT_ONE)
            .bind(schedule.as_str())
            .bind(fleet.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_READ))?;

        row.as_ref()
            .map(Schedule::from_row)
            .transpose()
            .map_err(error::query(CONTEXT_READ))
    }

    /// Creates a schedule, already claimed by the caller that will register it.
    ///
    /// The counts, the duplicate check and the insert run inside ONE
    /// transaction that has taken the fleet's row — see [`sql::LOCK_FLEET`] on
    /// why a lock on the parent is what makes a count-then-insert atomic.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a statement that failed, and
    /// a row this build cannot read. A bound the operator hit is `Ok(Err(..))`.
    pub async fn create(
        &self,
        workspace: &Uuid7,
        new: NewSchedule<'_>,
        token: &Uuid7,
        now: UnixMillis,
    ) -> Result<core::result::Result<Schedule, Refused>> {
        let schedule_id = self.mint(now)?;
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection
            .begin()
            .await
            .map_err(error::query(CONTEXT_WRITE))?;

        if let Some(refused) = admit::refusal(&mut transaction, workspace, &new).await? {
            return Ok(Err(refused));
        }

        let row = sqlx::query(sql::INSERT)
            .bind(schedule_id.as_str())
            .bind(new.fleet.as_str())
            .bind(new.source.as_str())
            .bind(new.source_key)
            .bind(new.cron)
            .bind(new.timezone)
            .bind(new.message)
            .bind(DesiredStatus::Active.as_str())
            .bind(SyncStatus::Syncing.as_str())
            // Generation ONE, never zero: the column's own CHECK refuses zero,
            // because "never synced" and "synced at generation zero" would
            // otherwise be the same state to a finalize.
            .bind(1_i64)
            .bind(token.as_str())
            .bind(now.as_millis() + SYNC_LEASE_MS)
            .bind(now.as_millis())
            .bind(new.once)
            .fetch_one(transaction.as_mut())
            .await
            .map_err(error::query(CONTEXT_WRITE))?;

        let created = Schedule::from_row(&row).map_err(error::query(CONTEXT_WRITE))?;
        transaction
            .commit()
            .await
            .map_err(error::query(CONTEXT_WRITE))?;
        Ok(Ok(created))
    }

    /// What a signed fire resolves to.
    ///
    /// `Ok(None)` for a schedule this daemon has no row for — a callback the
    /// external scheduler kept after the schedule was deleted. Dropped rather
    /// than refused: the sender is correctly configured and acting on what it
    /// was last told.
    ///
    /// # Errors
    /// As [`Self::list`].
    pub async fn fire_target(&self, schedule: &Uuid7) -> Result<Option<FireTarget>> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::FIRE_TARGET)
            .bind(schedule.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_READ))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let unreadable = error::query(CONTEXT_READ);
        let fleet: String = row.try_get(COLUMN_FLEET).map_err(&unreadable)?;
        let workspace: String = row.try_get(COLUMN_WORKSPACE).map_err(&unreadable)?;
        let message: String = row.try_get(COLUMN_MESSAGE).map_err(&unreadable)?;
        let once: bool = row.try_get(COLUMN_ONCE).map_err(&unreadable)?;
        let desired: String = row.try_get(COLUMN_DESIRED_STATUS).map_err(&unreadable)?;
        let fleet_status: String = row.try_get(COLUMN_FLEET_STATUS).map_err(&unreadable)?;

        Ok(Some(FireTarget {
            fleet: Uuid7::parse(&fleet).map_err(|_shape| error::row_unreadable(COLUMN_FLEET))?,
            workspace: Uuid7::parse(&workspace)
                .map_err(|_shape| error::row_unreadable(COLUMN_WORKSPACE))?,
            message,
            once,
            desired_status: DesiredStatus::parse(&desired)
                .ok_or_else(|| error::row_unreadable(COLUMN_DESIRED_STATUS))?,
            fleet_status,
        }))
    }
}
