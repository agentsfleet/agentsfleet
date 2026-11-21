//! Taking a draft's thread: winning its fence, expiring the lapsed run that
//! ran the turn before, and failing the turn once its runs have lapsed too
//! often.

use af_core::id::Uuid7;
use af_wire::setup::FailureReason;
use afd_setup::setup_run::{SetupOwed, SetupTurn};
use jiff::Timestamp;

use super::assign::CONTEXT;
use crate::error::{Result, query};
use crate::run::{RunOwner, RunService, sql};

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
pub(super) struct Taken {
    /// The thread and whether the assignment reassigns the turn; `None` when
    /// the draft yields nothing now.
    pub(super) won: Option<(Won, bool)>,
    /// The lapsed holder whose run the assignment expired, counted once the
    /// transaction commits.
    pub(super) expired: Option<Uuid7>,
}

impl RunService {
    /// Wins the draft's thread, bumping its fence, and expires the lapsed run
    /// that ran the turn before: the thread and whether this assignment reassigns the
    /// turn, and whose run it expired. No thread when a live runner still holds it,
    /// or when the turn's runs lapsed [`af_core::limits::EVENT_RUNS_MAX`] times,
    /// which fails it and frees the thread rather than reassigning it forever.
    pub(super) async fn take_thread(
        &self,
        tx: &mut sqlx::PgConnection,
        drafts: &afd_setup::Drafts,
        runner: &Uuid7,
        turn: &SetupTurn,
        owed: &mut SetupOwed,
    ) -> Result<Taken> {
        let owner = RunOwner::Draft(turn.draft_id.clone());
        let won: Option<Won> = sqlx::query_as(sql::shared::ASSIGN_THREAD)
            .bind(owner.fleet_id())
            .bind(owner.draft_id())
            .bind(runner)
            .bind(self.runs.clock().ttl())
            .bind(self.runs.clock().offline_after())
            .bind(sql::RUNNER_STATUS_ACTIVE)
            .fetch_optional(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        let Some(won) = won else {
            return Ok(Taken {
                won: None,
                expired: None,
            });
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
        if prior.earlier_runs < i64::from(af_core::limits::EVENT_RUNS_MAX) {
            let reassignment = prior.predecessor_run_id.is_some();
            return Ok(Taken {
                won: Some((won, reassignment)),
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
        Ok(Taken { won: None, expired })
    }
}
