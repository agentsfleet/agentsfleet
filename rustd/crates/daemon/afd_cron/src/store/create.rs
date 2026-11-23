//! Capacity is counted in the fleet's writer turn before intent and its fence
//! commit.

use super::history::{Fact, Receipt};
use super::{CONTEXT_WRITE, NewSchedule, Refused, SYNC_RUN, Schedules, admit, fire_at};
use crate::{DesiredStatus, Result, Schedule, SyncStatus, error, sql};
use af_core::id::Uuid7;
use afd_api_wire::activity::ScheduleChange;
use afd_db::Precondition;
use jiff::Timestamp;

/// A new schedule's fence version: one, never zero.
///
/// The column's own CHECK refuses zero, because "never synced" and "synced at
/// version zero" would otherwise be the same state to a finalize.
const FIRST_VERSION: i64 = 1;

impl Schedules {
    /// Creates a schedule, already claimed by the caller that will register it.
    ///
    /// The counts, the duplicate check and the insert run inside ONE
    /// transaction that has taken the fleet's writer turn — see
    /// [`sql::LOCK_FLEET`] on why that is what makes a count-then-insert
    /// atomic.
    ///
    /// `now` mints the new row's id and nothing else: Postgres stamps the row,
    /// and a `once` schedule's instant is counted from that stamp.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a statement that failed, and
    /// a row this build cannot read. A bound the operator hit is `Ok(Err(..))`.
    pub async fn create(
        &self,
        workspace: &Uuid7,
        new: NewSchedule<'_>,
        token: &Uuid7,
        now: Timestamp,
    ) -> Result<core::result::Result<Schedule, Refused>> {
        self.insert(workspace, None, new, token, now, None).await
    }

    /// [`Self::create`], proving `guard` after the fleet's turn is taken and
    /// before the insert, so a guard that no longer holds writes nothing.
    ///
    /// `keyed` is the row's id when the caller names one: a fleet's create is
    /// keyed by its call's item, so a retry whose first answer was lost finds
    /// the row the first attempt wrote, claims it for the push that follows,
    /// and writes no second schedule ([`Self::earlier`]). `None` mints one.
    pub(super) async fn insert(
        &self,
        workspace: &Uuid7,
        keyed: Option<&Uuid7>,
        new: NewSchedule<'_>,
        token: &Uuid7,
        now: Timestamp,
        guard: Option<&dyn Precondition>,
    ) -> Result<core::result::Result<Schedule, Refused>> {
        let schedule_id = match keyed {
            Some(id) => id.clone(),
            None => self.mint(now)?,
        };
        let mut transaction = sqlx::Transaction::begin(self.database.acquire().await?, None)
            .await
            .map_err(error::query(CONTEXT_WRITE))?;

        // Before the counts: the row a first attempt wrote is already one of
        // them, and its retry must not be refused for it.
        if keyed.is_some()
            && let Some(earlier) = Self::earlier(
                &mut transaction,
                workspace,
                &schedule_id,
                &new,
                token,
                guard,
            )
            .await?
        {
            return commit_earlier(transaction, earlier).await;
        }
        if let Some(refused) = admit::refusal(&mut transaction, workspace, &new).await? {
            return Ok(Err(refused));
        }
        if let Some(guard) = guard
            && !guard
                .holds(&mut transaction)
                .await
                .map_err(error::query(CONTEXT_WRITE))?
        {
            return Ok(Err(Refused::Unheld));
        }

        let Some(created) = insert_row(&mut transaction, &new, &schedule_id, token).await? else {
            // The key was taken after the read above; only a keyed create can
            // still find its own row behind it.
            let earlier = match keyed {
                Some(_) => {
                    Self::earlier(
                        &mut transaction,
                        workspace,
                        &schedule_id,
                        &new,
                        token,
                        guard,
                    )
                    .await?
                }
                None => None,
            };
            return commit_earlier(transaction, earlier.unwrap_or(Err(Refused::DuplicateKey)))
                .await;
        };
        let created = if created.once {
            fire_at::stamp(&mut transaction, &created, token, created.created_at).await?
        } else {
            created
        };

        let announcement = Receipt {
            schedule: &created,
            fact: Fact::Changed(ScheduleChange::Created),
        }
        .record_on(&mut transaction, workspace)
        .await?;
        self.commit_activity(transaction, announcement).await?;
        Ok(Ok(created))
    }

    /// The row an earlier attempt of a keyed create wrote, claimed for this
    /// attempt's push, or `None` when there is none.
    ///
    /// A row another source holds under the key is refused as a duplicate,
    /// so a fleet never answers a person's schedule as its own. One a live
    /// syncer holds is answered unclaimed, and the push that follows reads
    /// it as held by another.
    async fn earlier(
        transaction: &mut sqlx::PgConnection,
        workspace: &Uuid7,
        schedule_id: &Uuid7,
        new: &NewSchedule<'_>,
        token: &Uuid7,
        guard: Option<&dyn Precondition>,
    ) -> Result<Option<core::result::Result<Schedule, Refused>>> {
        let in_workspace = sqlx::query(sql::FLEET_IN_WORKSPACE)
            .bind(new.fleet)
            .bind(workspace)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(error::query(CONTEXT_WRITE))?;
        if in_workspace.is_none() {
            return Ok(Some(Err(Refused::NoSuchFleet)));
        }
        let Some(row) = sqlx::query_as::<_, Schedule>(sql::SELECT_ONE)
            .bind(schedule_id)
            .bind(new.fleet)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(error::query(CONTEXT_WRITE))?
        else {
            return Ok(None);
        };
        if row.source != new.source {
            return Ok(Some(Err(Refused::DuplicateKey)));
        }
        if let Some(guard) = guard
            && !guard
                .holds(&mut *transaction)
                .await
                .map_err(error::query(CONTEXT_WRITE))?
        {
            return Ok(Some(Err(Refused::Unheld)));
        }
        let claimed = sqlx::query_as::<_, Schedule>(sql::CLAIM_CURRENT)
            .bind(schedule_id)
            .bind(new.fleet)
            .bind(SyncStatus::Syncing.as_str())
            .bind(token)
            .bind(SYNC_RUN)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(error::query(CONTEXT_WRITE))?;
        Ok(Some(Ok(claimed.unwrap_or(row))))
    }
}

/// Commits what [`Schedules::earlier`] answered: a claim it took lands, and
/// a refusal wrote nothing to keep.
async fn commit_earlier(
    transaction: sqlx::Transaction<'_, sqlx::Postgres>,
    earlier: core::result::Result<Schedule, Refused>,
) -> Result<core::result::Result<Schedule, Refused>> {
    transaction
        .commit()
        .await
        .map_err(error::query(CONTEXT_WRITE))?;
    Ok(earlier)
}

/// The new row, or `None` when an existing one holds its id or its
/// scheduler key: the insert does nothing on a conflict.
async fn insert_row(
    connection: &mut sqlx::PgConnection,
    new: &NewSchedule<'_>,
    schedule_id: &Uuid7,
    token: &Uuid7,
) -> Result<Option<Schedule>> {
    sqlx::query_as::<_, Schedule>(sql::INSERT)
        .bind(schedule_id)
        .bind(new.fleet)
        .bind(new.source.as_str())
        .bind(
            new.provider_schedule_id
                .unwrap_or_else(|| schedule_id.as_str()),
        )
        .bind(new.cron)
        .bind(new.timezone)
        .bind(new.message)
        .bind(DesiredStatus::Active.as_str())
        .bind(SyncStatus::Syncing.as_str())
        .bind(FIRST_VERSION)
        .bind(token)
        .bind(SYNC_RUN)
        .bind(new.once)
        .fetch_optional(connection)
        .await
        .map_err(error::query(CONTEXT_WRITE))
}
