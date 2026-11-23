//! Taking a draft's thread: winning its fence, expiring the lapsed run that
//! ran the turn before, and failing the turn once its runs have lapsed too
//! often.
//!
//! The thread claim is the one decision that two passes over one draft meet
//! at: `ASSIGN_THREAD` is a conditional upsert on the thread row, keyed by the
//! draft's id, and only one pass gets its row back. No lock on the draft or
//! its creator stands in front of it.

use af_core::id::Uuid7;
use af_wire::setup::FailureReason;
use afd_db::constraint::violates_foreign_key;
use afd_setup::setup_run::{SetupOwed, SetupTurn};
use jiff::Timestamp;

use super::assign::CONTEXT;
use crate::error::{Result, query};
use crate::run::{RunOwner, RunService, sql};

/// The key a draft's thread holds on its draft. A draft deleted since its
/// facts were read took its thread with it, and the claim's insert of a new
/// thread names a draft that is gone.
const THREAD_DRAFT_FK: &str = "threads_draft_id_fkey";

/// [`sql::shared::ASSIGN_THREAD`]'s winning row, by column name.
#[derive(sqlx::FromRow)]
pub(super) struct Won {
    pub(super) fencing_token: i64,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub(super) run_expires_at: Timestamp,
}

/// [`sql::shared::EXPIRE_PRIOR_RUN`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct Prior {
    predecessor_run_id: Option<Uuid7>,
    expired_runner_id: Option<Uuid7>,
    earlier_runs: i64,
}

/// What taking a draft's thread came to.
pub(super) enum Taken {
    /// The assignment holds the thread.
    Won {
        won: Won,
        /// The turn ran before, under a run that lapsed.
        reassignment: bool,
        /// The lapsed holder whose run the assignment expired, counted once
        /// the transaction commits.
        expired: Option<Uuid7>,
    },
    /// The draft yields nothing now: a live runner holds its thread, or its
    /// turn's runs lapsed too often and the turn is failed. What this wrote
    /// commits.
    Passed { expired: Option<Uuid7> },
    /// The draft was deleted since its facts were read. Its foreign key
    /// refused the claim, so the transaction is spent: nothing commits.
    Gone,
}

impl RunService {
    /// Wins the draft's thread, bumping its fence, and expires the lapsed run
    /// that ran the turn before: the thread and whether this assignment reassigns the
    /// turn, and whose run it expired. No thread when a live runner still holds it,
    /// or when the turn's runs lapsed the deployment's runs per turn,
    /// which fails it and frees the thread rather than reassigning it forever.
    /// [`Taken::Gone`] when the draft was deleted since its facts were read.
    pub(super) async fn take_thread(
        &self,
        tx: &mut sqlx::PgConnection,
        drafts: &afd_setup::Drafts,
        runner: &Uuid7,
        turn: &SetupTurn,
        owed: &mut SetupOwed,
    ) -> Result<Taken> {
        let owner = RunOwner::Draft(turn.draft_id.clone());
        let claimed: core::result::Result<Option<Won>, sqlx::Error> =
            sqlx::query_as(sql::shared::ASSIGN_THREAD)
                .bind(owner.fleet_id())
                .bind(owner.draft_id())
                .bind(runner)
                .bind(self.runs.clock().ttl())
                .bind(self.runs.clock().offline_after())
                .bind(sql::RUNNER_STATUS_ACTIVE)
                .fetch_optional(&mut *tx)
                .await;
        let won = match claimed {
            Ok(won) => won,
            Err(error) if violates_foreign_key(&error, THREAD_DRAFT_FK) => return Ok(Taken::Gone),
            Err(error) => return Err(query(CONTEXT)(error)),
        };
        let Some(won) = won else {
            return Ok(Taken::Passed { expired: None });
        };
        let prior: Prior = sqlx::query_as(sql::shared::EXPIRE_PRIOR_RUN)
            .bind(owner.fleet_id())
            .bind(owner.draft_id())
            .bind(&turn.message_id)
            .bind(sql::RUN_STATUS_ACTIVE)
            .bind(sql::RUN_STATUS_EXPIRED)
            .fetch_one(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        let expired = prior.expired_runner_id;
        if prior.earlier_runs < self.runs.limits().runs_per_turn() {
            let reassignment = prior.predecessor_run_id.is_some();
            return Ok(Taken::Won {
                won,
                reassignment,
                expired,
            });
        }
        // Every earlier run lapsed unreported: the turn is lost, not retried forever.
        drafts
            .fail_setup_turn(&mut *tx, turn, FailureReason::ProviderUsageUnknown, owed)
            .await?;
        sqlx::query(sql::shared::RELEASE_RUNLESS_THREAD)
            .bind(owner.thread_id())
            .bind(won.fencing_token)
            .execute(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        Ok(Taken::Passed { expired })
    }
}
