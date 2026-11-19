//! The one transaction a terminal report commits, and what it answers.
//!
//! A plain chat's report is one statement ([`crate::run::sql::report_statement`]):
//! the metered slice and its lines, the run and admission settled, the
//! event closed, the session checkpointed, the slot released, the delivery
//! owed and the audit row, under the run's own row lock and no host lock
//!. A durable task's settlement and a run's folds stay statements
//! of their own in the same transaction, after it: the task's settlement reads
//! the runtime the statement metered, and takes the task row after the run
//! row as a renewal does. Dropping the transaction on any error rolls every
//! write back.

use af_core::id::Uuid7;
use afd_billing::Nanos;
use afd_billing::biller::Settlement;
use afd_turns::view::Closed;
use jiff::Timestamp;
use sqlx::{Acquire as _, PgConnection};

use crate::error::{Result, query};
use crate::run::delivery::Owing;
use crate::run::fold::Folds;
use crate::run::report::RunTokens;
use crate::run::settle::Reported;
use crate::run::sql::report_statement::{ReportAnswer, ReportStatement, SettledReport};
use crate::run::store::Runs;
use crate::run::unanswered;
use crate::run::usage::Pricing;
use crate::run::verdict::Terminal;

mod steps;

use self::steps::{Step, apply_notice, next_step, release, warn_unknown_reply};

/// What a query failure names this transaction as.
const CONTEXT_COMMIT: &str = "report commit";

/// Everything one terminal report writes.
#[derive(Debug)]
pub struct TerminalReport<'a> {
    /// The run being reported.
    pub run_id: &'a Uuid7,
    /// The runner that holds it.
    pub runner_id: &'a Uuid7,
    /// The run as the report read it: its fleet, workspace, payer and
    /// event, and the pricing facts its lines are guarded on.
    pub run: &'a Reported,
    /// The run's identifier as the work its usage lines name.
    pub run_key: &'a Uuid7,
    /// One line per provider request no renewal acknowledged.
    pub requests: &'a [af_wire::usage::RequestUsage],
    /// What the final slice is priced against.
    pub pricing: &'a Pricing,
    /// The run's ending, as the event row will carry it.
    pub outcome: Terminal<'a>,
    /// The event the fleet's next run resumes after.
    pub last_turn_id: &'a Uuid7,
    /// The answer that run resumes from.
    pub last_response: &'a str,
    /// When the sandbox the run left held for the fleet's next event lapses;
    /// none when the runner holds nothing.
    pub held_until: Option<Timestamp>,
    /// Native task participant, prepared before taking report locks.
    pub task_settlement: Option<crate::task::TaskSettlement<'a>>,
    /// The last folded message a completed model call read; its messages
    /// settle with this answer when the run succeeded.
    pub folded_through_sequence: Option<i64>,
    /// The instant every identifier this transaction mints is drawn at.
    pub now: Timestamp,
}

/// What the report's transaction decided.
#[derive(Debug)]
pub enum Committed {
    /// The report won its fence and every write committed.
    Settled {
        /// Authoritative retained sandbox deadline, absent for late reports.
        accepted_hold: Option<Timestamp>,
        /// What the report's lines charged, its runtime line included.
        charged: Nanos,
        /// The run's token sums, this report's lines included.
        tokens: RunTokens,
        /// The row this report ended, for the frame the caller announces;
        /// `None` when the event was already terminal.
        closed: Option<Box<Closed>>,
        /// The delivery delivery this report newly owed, for the caller to
        /// wake the outbound workers about.
        owed: Option<Owing>,
        /// The task's notice, when its settlement replaced the answer.
        notice: Option<&'static str>,
        /// The folds this answer settled.
        folds: Folds,
    },
    /// This runner settled the run before; nothing was written.
    AlreadySettled,
    /// The run's fence moved; only the runner's known request lines were
    /// logged.
    Fenced {
        /// Those request lines, logged and still to be settled.
        settlement: Option<Box<Settlement>>,
    },
}

impl Runs {
    /// Commits a terminal report.
    ///
    /// A statement priced under facts the run no longer holds — its
    /// catalogue moved between the price read and the statement — writes
    /// nothing, and is run once more under the run's own facts.
    ///
    /// # Errors
    /// Reports an entropy source that could not mint an identifier and a
    /// datastore that would not answer, in which case nothing was written.
    pub async fn commit_report(&self, mut report: TerminalReport<'_>) -> Result<Committed> {
        let mut task_settlement = report.task_settlement.take();
        let mut repriced: Option<Pricing> = None;
        loop {
            let pricing = repriced.as_ref().unwrap_or(report.pricing);
            let mut connection = self.pool().acquire().await?;
            let mut transaction = connection.begin().await.map_err(query(CONTEXT_COMMIT))?;
            let answer = self
                .run_report_statement(&mut transaction, &report, pricing)
                .await?;
            match next_step(answer, repriced.is_some()) {
                Step::Settle(settled) => {
                    return self
                        .settled(transaction, &report, *settled, task_settlement.take())
                        .await;
                }
                Step::AlreadySettled => return Ok(Committed::AlreadySettled),
                Step::Reprice(held) => {
                    let posture = Pricing::posture_of(&held.posture)?;
                    release(transaction).await?;
                    // Back to the pool before the book read takes one, so a
                    // report never holds two of a small pool's connections.
                    drop(connection);
                    repriced = Some(
                        Pricing::load(
                            self.pool(),
                            posture,
                            &held.provider,
                            &held.model,
                            report.run_id.as_str(),
                        )
                        .await,
                    );
                }
                Step::KnownRequestsOnly => {
                    release(transaction).await?;
                    drop(connection);
                    let settlement = self
                        .record_known_requests(
                            report.runner_id,
                            report.run_key,
                            pricing,
                            report.requests,
                            report.now,
                        )
                        .await?;
                    return Ok(Committed::Fenced {
                        settlement: settlement.map(Box::new),
                    });
                }
            }
        }
    }

    /// The one report statement, priced under `pricing`.
    async fn run_report_statement(
        &self,
        connection: &mut PgConnection,
        report: &TerminalReport<'_>,
        pricing: &Pricing,
    ) -> Result<ReportAnswer> {
        let now = report.now;
        let lines = pricing.request_lines(report.run.run_created_at, now, report.requests)?;
        let line_ids = lines
            .iter()
            .map(|_line| self.entropy().uuid7(now))
            .collect::<core::result::Result<Vec<_>, _>>()?;
        let [runtime_line_id, delivery_id, delivery_receipt, audit_id] =
            [(); 4].map(|()| self.entropy().uuid7(now));
        let (runtime_line_id, delivery_id, delivery_receipt, audit_id) =
            (runtime_line_id?, delivery_id?, delivery_receipt?, audit_id?);
        let outcome = &report.outcome;
        let checkpoint =
            crate::run::finalize::checkpoint_document(report.last_turn_id, report.last_response);
        let run = report.run;
        let failure_reason = outcome.verdict.label();
        let notice = (!outcome.verdict.succeeded())
            .then(|| unanswered::failed_run(&run.fleet_id, failure_reason.as_deref()));
        let (host_digest, host_connection) = self.host_proof(report.runner_id);
        ReportStatement {
            run_id: report.run_id,
            runner_id: report.runner_id,
            fleet_id: &run.fleet_id,
            workspace_id: &run.workspace_id,
            succeeded: outcome.verdict.succeeded(),
            posture: pricing.posture().as_str(),
            provider: pricing.provider(),
            model: pricing.model(),
            catalogue_version: pricing.catalogue_version(),
            lines: &lines,
            line_ids: &line_ids,
            runtime_price: pricing.runtime_price(now),
            runtime_line_id: &runtime_line_id,
            status: outcome.verdict.status(),
            response_text: outcome.response_text,
            tokens: outcome.tokens,
            wall_ms: outcome.wall_ms,
            failure_reason,
            failure_detail: outcome.verdict.detail(),
            tool_calls: outcome.tool_calls,
            checkpoint: &checkpoint,
            held_until: report.held_until,
            sandbox_idle_hold: self.clock().sandbox_idle_hold(),
            host_digest,
            host_connection,
            delivery_id: &delivery_id,
            delivery_receipt: &delivery_receipt,
            audit_id: &audit_id,
            audit_metadata: serde_json::json!({
                afd_runner::sql::meta::RUN_ID: report.run_id.as_str(),
                afd_runner::sql::meta::FLEET_ID: run.fleet_id.as_str(),
                afd_runner::sql::meta::AGENTSFLEET_EVENT_ID: run.turn_id.as_str(),
            }),
            unanswered_notice: notice.as_deref(),
        }
        .run(connection)
        .await
    }

    /// A settled report: a durable task's model spend and settlement, the
    /// folds, then the commit.
    async fn settled(
        &self,
        mut transaction: sqlx::Transaction<'_, sqlx::Postgres>,
        report: &TerminalReport<'_>,
        settled: SettledReport,
        task_settlement: Option<crate::task::TaskSettlement<'_>>,
    ) -> Result<Committed> {
        let mut notice = None;
        if settled.has_task {
            crate::task::usage::record(&mut transaction, report.run_key, settled.model_nanos)
                .await?;
            notice = crate::task::settle_report(
                &mut transaction,
                report.run_key,
                report.runner_id,
                task_settlement,
            )
            .await?;
            if let Some(notice) = notice {
                let owed = (
                    self.entropy().uuid7(report.now)?,
                    self.entropy().uuid7(report.now)?,
                );
                apply_notice(&mut transaction, report, notice, &owed).await?;
            }
        }
        if settled.owed.is_none() {
            warn_unknown_reply(report.run, settled.reply_provider.as_deref());
        }
        let succeeded = report.outcome.verdict.succeeded();
        // A failed or stopped run's folds settle nothing: each message stays
        // for the fleet's next run, in order.
        let read_through = report.folded_through_sequence.filter(|_| succeeded);
        let answer = notice.unwrap_or(report.outcome.response_text);
        let folds = self
            .settle_folds(
                &mut transaction,
                report.run,
                report.run_id,
                read_through,
                answer,
            )
            .await?;
        transaction.commit().await.map_err(query(CONTEXT_COMMIT))?;
        Ok(Committed::Settled {
            accepted_hold: settled.accepted_hold,
            charged: settled.charged,
            tokens: settled.tokens,
            closed: settled.closed.map(Box::new),
            owed: settled.owed.map(|delivery| Owing { delivery }),
            notice,
            folds,
        })
    }
}
