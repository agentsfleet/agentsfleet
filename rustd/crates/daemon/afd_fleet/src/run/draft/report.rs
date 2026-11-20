//! A draft run's report: one transaction locks the draft, settles the run
//! with the shared settlement, logs the final usage lines and finishes the
//! draft's message with at most one candidate.
//!
//! Idempotent by run and fence: the settlement flips the run only while it
//! is active under its own fence, so a replayed report finds it settled, rolls
//! back and writes nothing a second time. A stale fence is refused and writes
//! nothing; the dead holder's last slice is the expiry sweep's to bill.

use af_core::id::Uuid7;
use af_wire::report::{Outcome, ReportRequest};
use garde::Validate as _;
use jiff::Timestamp;
use sqlx::FromRow as _;

use super::renew::requested;
use crate::error::{Result, query, run_lost, run_not_found, stale_fence, wire_run};
use crate::run::usage::{Pricing, SliceColumns};
use crate::run::{RunService, sql};

/// What query failures name a draft report as.
const CONTEXT: &str = "settle a draft run report";

/// [`sql::shared::SETTLE_RUN`]'s verdict columns, by column name.
#[derive(sqlx::FromRow)]
struct Settled {
    moved_runs: i64,
    run_status: Option<String>,
    fence_holds: Option<bool>,
}

impl RunService {
    /// Records `runner`'s report for a draft-owned run: the run settled,
    /// its last usage lines logged, and the draft's message finished, with one
    /// candidate when the reported setup still stands on the draft's head.
    ///
    /// # Errors
    /// Refuses a run that is not this runner's draft run, a report for
    /// another message, and a superseded fence, each writing nothing. A
    /// replayed report answers success and writes nothing. Reports a datastore
    /// that would not answer.
    pub async fn report_draft(
        &self,
        creations: &afd_setup::Creations,
        runner: &Uuid7,
        request: &ReportRequest<'_>,
        now: Timestamp,
    ) -> Result<()> {
        let run_id = wire_run(request.run_id.as_ref())?;
        let setup = request.setup.as_ref().ok_or_else(run_not_found)?;
        setup.validate()?;
        let Some(run) = self.runs.draft_run(&run_id, runner).await? else {
            return Err(run_not_found());
        };
        if request.turn_id.as_ref() != run.turn_id.as_str() {
            return Err(run_not_found());
        }
        if i64::try_from(request.fencing_token).ok() != Some(run.fencing_token) {
            return Err(stale_fence());
        }
        let pricing = Pricing::load(
            self.runs.pool(),
            Pricing::posture_of(&run.posture)?,
            &run.provider,
            &run.model,
            run_id.as_str(),
        )
        .await;
        // Owned by the transaction, so its commit can announce after it.
        let mut tx = sqlx::Transaction::begin(self.runs.pool().acquire().await?, None)
            .await
            .map_err(query(CONTEXT))?;
        // The owner first, then the thread and the run inside the settlement.
        let scope = creations
            .lock_setup_report(&mut tx, &run.draft_id, &run.turn_id)
            .await?;
        let succeeded = request.outcome == Outcome::Processed;
        let audit_id = self.runs.entropy().uuid7(now)?;
        let metadata = serde_json::json!({
            (afd_runner::sql::meta::RUN_ID): run_id.as_str(),
            (afd_runner::sql::meta::AGENTSFLEET_EVENT_ID): run.turn_id.as_str(),
        });
        let (digest, connection_id) = self.runs.host_proof(runner);
        let row = sqlx::query(sql::shared::SETTLE_RUN)
            .bind(&run_id)
            .bind(runner)
            .bind(afd_billing::nanos::MS_PER_SEC)
            .bind(sql::RUN_STATUS_ACTIVE)
            .bind(sql::report_statement::settled_status(
                request.outcome == Outcome::Stopped,
            ))
            .bind(succeeded)
            // A draft's run holds no sandbox, so its report asks for no hold.
            .bind(None::<jiff_sqlx::Timestamp>)
            .bind(self.runs.clock().sandbox_idle_hold())
            .bind(&audit_id)
            .bind(afd_runner::sql::event_type::RUN_RELEASED)
            .bind(sqlx::types::Json(&metadata))
            .bind(i64::from(af_wire::usage::RUNTIME_MS_MAX))
            .bind(digest)
            .bind(connection_id)
            .bind(None::<Vec<String>>)
            .fetch_one(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        let settled = Settled::from_row(&row).map_err(query(CONTEXT))?;
        if settled.moved_runs == 0 {
            drop(tx);
            return match settled.run_status.as_deref() {
                // Settled already, by this report's earlier copy: nothing more to write.
                Some(status) if sql::report_statement::is_settled(status) => Ok(()),
                _ if settled.fence_holds == Some(false) => Err(stale_fence()),
                _ => Err(run_lost()),
            };
        }
        let requests = &request.requests.0;
        let settlement = match SliceColumns::from_row(&row)
            .map_err(query(CONTEXT))?
            .into_draft_slice()
        {
            Some(slice) => pricing.draft_settlement(&run_id, slice, requests)?,
            None => requested(&pricing, &run_id, &run, requests, now)?,
        };
        self.log_draft(&mut tx, settlement.as_ref(), now).await?;
        let mut owed = afd_setup::setup_run::SetupOwed::default();
        let announcement = match &scope {
            Some(scope) => {
                let answer = Some(request.response_text.as_ref()).filter(|text| !text.is_empty());
                let finish = afd_setup::setup_run::SetupFinish {
                    completed: succeeded,
                    answer,
                    proposal: setup.proposal.as_ref(),
                    now,
                };
                Box::pin(creations.finish_setup_turn(&mut tx, scope, finish, &mut owed)).await?
            }
            None => afd_turns::view::activity::Announcement::default(),
        };
        creations
            .commit_setup_report(tx, announcement, owed)
            .await?;
        if let Some(scope) = &scope {
            Box::pin(creations.after_setup_report(scope)).await?;
        }
        Ok(())
    }
}
