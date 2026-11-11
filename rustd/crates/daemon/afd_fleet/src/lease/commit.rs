//! The one transaction a terminal report commits, and what it answers.
//!
//! A plain chat's report is one statement ([`crate::lease::sql::report_statement`]):
//! the metered slice and its lines, the lease and admission settled, the
//! event closed, the session checkpointed, the slot released, the obligation
//! owed and the audit row, under the lease's own row lock and no host lock
//! (M245 §6). A durable task's settlement and a run's folds stay statements
//! of their own in the same transaction, after it: the task's settlement reads
//! the runtime the statement metered, and takes the task row after the lease
//! row as a renewal does. Dropping the transaction on any error rolls every
//! write back.

use af_core::id::Uuid7;
use afd_billing::Nanos;
use afd_billing::biller::Settlement;
use afd_events::Closed;
use jiff::Timestamp;
use sqlx::{Acquire as _, PgConnection};

use crate::error::{Result, query};
use crate::lease::fold::Folds;
use crate::lease::obligation::Owing;
use crate::lease::report::RunTokens;
use crate::lease::settle::Reported;
use crate::lease::sql::report_statement::{
    HeldFacts, ReportAnswer, ReportStatement, SettledReport,
};
use crate::lease::store::Leases;
use crate::lease::usage::Pricing;
use crate::lease::verdict::Terminal;

/// What a query failure names this transaction as.
const CONTEXT_COMMIT: &str = "report commit";

/// Logged when a stored reply connector names no connector this build knows.
const EVENT_REPLY_PROVIDER_UNKNOWN: &str = "report_reply_provider_unknown";

/// Everything one terminal report writes.
#[derive(Debug)]
pub struct TerminalReport<'a> {
    /// The lease being reported.
    pub lease_id: &'a Uuid7,
    /// The runner that holds it.
    pub runner_id: &'a Uuid7,
    /// The lease as the report read it: its fleet, workspace, payer and
    /// event, and the pricing facts its lines are guarded on.
    pub lease: &'a Reported,
    /// The lease's identifier as the work its usage lines name.
    pub lease_key: &'a Uuid7,
    /// One line per provider request no renewal acknowledged.
    pub requests: &'a [af_wire::usage::RequestUsage],
    /// What the final slice is priced against.
    pub pricing: &'a Pricing,
    /// The run's ending, as the event row will carry it.
    pub outcome: Terminal<'a>,
    /// The event the fleet's next run resumes after.
    pub last_event_id: &'a Uuid7,
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
        /// What the report's lines charged, its runtime line included.
        charged: Nanos,
        /// The run's token sums, this report's lines included.
        tokens: RunTokens,
        /// The row this report ended, for the frame the caller announces;
        /// `None` when the event was already terminal.
        closed: Option<Box<Closed>>,
        /// The delivery obligation this report newly owed, for the caller to
        /// wake the outbound workers about.
        owed: Option<Owing>,
        /// The task's notice, when its settlement replaced the answer.
        notice: Option<&'static str>,
        /// The folds this answer settled.
        folds: Folds,
    },
    /// This runner settled the lease before; nothing was written.
    AlreadySettled,
    /// The lease's fence moved; only the runner's known request lines were
    /// logged (blocker 4).
    Fenced {
        /// Those request lines, logged and still to be settled.
        settlement: Option<Box<Settlement>>,
    },
}

impl Leases {
    /// Commits a terminal report.
    ///
    /// A statement priced under facts the lease no longer holds — its
    /// catalogue moved between the price read and the statement — writes
    /// nothing, and is run once more under the lease's own facts.
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
                    release(transaction).await?;
                    repriced = Some(
                        Pricing::load(
                            self.pool(),
                            Pricing::posture_of(&held.posture),
                            &held.provider,
                            &held.model,
                            report.lease_id.as_str(),
                        )
                        .await,
                    );
                }
                Step::KnownRequestsOnly => {
                    release(transaction).await?;
                    let settlement = self
                        .record_known_requests(
                            report.runner_id,
                            report.lease_key,
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
        let lines = pricing.request_lines(report.lease.lease_created_at, now, report.requests)?;
        let line_ids = lines
            .iter()
            .map(|_line| self.entropy().uuid7(now))
            .collect::<core::result::Result<Vec<_>, _>>()?;
        let [runtime_line_id, obligation_id, obligation_receipt, audit_id] =
            [(); 4].map(|()| self.entropy().uuid7(now));
        let (runtime_line_id, obligation_id, obligation_receipt, audit_id) = (
            runtime_line_id?,
            obligation_id?,
            obligation_receipt?,
            audit_id?,
        );
        let outcome = &report.outcome;
        let answer = outcome.response_text;
        let checkpoint =
            crate::lease::finalize::checkpoint_document(report.last_event_id, report.last_response);
        let lease = report.lease;
        ReportStatement {
            lease_id: report.lease_id,
            runner_id: report.runner_id,
            fleet_id: &lease.fleet_id,
            workspace_id: &lease.workspace_id,
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
            response_text: answer,
            tokens: outcome.tokens,
            wall_ms: outcome.wall_ms,
            failure_reason: outcome.verdict.label(),
            failure_detail: outcome.verdict.detail(),
            tool_calls: outcome.tool_calls,
            checkpoint: &checkpoint,
            held_until: report.held_until,
            obligation_id: &obligation_id,
            obligation_receipt: &obligation_receipt,
            answer,
            audit_id: &audit_id,
            audit_metadata: serde_json::json!({
                afd_runner::sql::meta::LEASE_ID: report.lease_id.as_str(),
                afd_runner::sql::meta::FLEET_ID: lease.fleet_id.as_str(),
                afd_runner::sql::meta::AGENTSFLEET_EVENT_ID: lease.event_id.as_str(),
            }),
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
            crate::task::usage::record(&mut transaction, report.lease_key, settled.model_nanos)
                .await?;
            notice = crate::task::settle_report(
                &mut transaction,
                report.lease_key,
                report.runner_id,
                task_settlement,
            )
            .await?;
            if let Some(notice) = notice {
                apply_notice(&mut transaction, report, notice).await?;
            }
        }
        if settled.owed.is_none() {
            warn_unknown_reply(report.lease, settled.reply_provider.as_deref());
        }
        let succeeded = report.outcome.verdict.succeeded();
        // A failed or stopped run's folds settle nothing: each message stays
        // for the fleet's next run, in order.
        let read_through = report.folded_through_sequence.filter(|_| succeeded);
        let answer = notice.unwrap_or(report.outcome.response_text);
        let folds = self
            .settle_folds(
                &mut transaction,
                report.lease,
                report.lease_id,
                read_through,
                answer,
            )
            .await?;
        transaction.commit().await.map_err(query(CONTEXT_COMMIT))?;
        Ok(Committed::Settled {
            charged: settled.charged,
            tokens: settled.tokens,
            closed: settled.closed.map(Box::new),
            owed: settled.owed.map(|obligation| Owing { obligation }),
            notice,
            folds,
        })
    }
}

/// What a report does after one run of its statement.
#[derive(Debug)]
enum Step {
    /// Every write landed; the task and folds follow in the transaction.
    Settle(Box<SettledReport>),
    /// This runner settled the lease before.
    AlreadySettled,
    /// The lease holds pricing facts other than the ones bound: price again
    /// under these and rerun, once.
    Reprice(HeldFacts),
    /// The fence moved, or the catalogue moved again under the rerun: only the
    /// runner's known request lines are logged (blocker 4), and the lease is
    /// left to its own renewal or expiry.
    KnownRequestsOnly,
}

/// The step after `answer`, when `repriced` says whether this is the rerun.
///
/// A catalogue edited between the price read and the statement costs one
/// rerun, for every lease (the spec's failure table). A second miss means a
/// second edit inside one report; settling under a third read would not be
/// more right, so the report stops at its known lines.
fn next_step(answer: ReportAnswer, repriced: bool) -> Step {
    match answer {
        ReportAnswer::Settled(settled) => Step::Settle(settled),
        ReportAnswer::AlreadySettled => Step::AlreadySettled,
        ReportAnswer::Repriced(held) if !repriced => Step::Reprice(held),
        ReportAnswer::Repriced(_) | ReportAnswer::Fenced => Step::KnownRequestsOnly,
    }
}

/// Ends a statement that wrote nothing before the caller takes another
/// connection: a dropped transaction's rollback waits for its connection's
/// next use, and the connection is still held, so the slot and lease locks it
/// took would block the caller's own next statement on the same lease.
async fn release(transaction: sqlx::Transaction<'_, sqlx::Postgres>) -> Result<()> {
    transaction.rollback().await.map_err(query(CONTEXT_COMMIT))
}

/// Puts a task's notice where the run's answer was: the event, the session
/// and an answer still owed.
async fn apply_notice(
    connection: &mut PgConnection,
    report: &TerminalReport<'_>,
    notice: &str,
) -> Result<()> {
    let checkpoint = crate::lease::finalize::checkpoint_document(report.last_event_id, notice);
    let _events: i64 = sqlx::query_scalar(crate::lease::sql::report_statement::APPLY_REPORT_NOTICE)
        .bind(&report.lease.fleet_id)
        .bind(&report.lease.event_id)
        .bind(notice)
        .bind(&checkpoint)
        .fetch_one(connection)
        .await
        .map_err(query(CONTEXT_COMMIT))?;
    Ok(())
}

/// Logs a stored reply connector no connector of this build names: the
/// answer is owed nowhere, as the read used to log.
fn warn_unknown_reply(lease: &Reported, stored: Option<&str>) {
    let Some(stored) = stored else {
        return;
    };
    if afd_connector::Provider::parse(stored).is_some() {
        return;
    }
    let fleet_id = lease.fleet_id.as_str();
    let agentsfleet_event_id = lease.event_id.as_str();
    tracing::warn!(
        fleet_id,
        agentsfleet_event_id,
        event = EVENT_REPLY_PROVIDER_UNKNOWN,
        "the recorded connector names no connector; the answer is owed nowhere"
    );
}

#[cfg(test)]
mod tests {
    use super::{Step, next_step};
    use crate::lease::report::RunTokens;
    use crate::lease::sql::report_statement::{HeldFacts, ReportAnswer, SettledReport};

    fn held() -> HeldFacts {
        HeldFacts {
            posture: "platform".to_owned(),
            provider: "anthropic".to_owned(),
            model: "claude-opus-5-5".to_owned(),
            catalogue_version: 2,
        }
    }

    fn settled() -> ReportAnswer {
        ReportAnswer::Settled(Box::new(SettledReport {
            has_task: false,
            closed: None,
            owed: None,
            reply_provider: None,
            tokens: RunTokens::default(),
            charged: afd_billing::Nanos::from_i64(0),
            model_nanos: afd_billing::Nanos::from_i64(0),
        }))
    }

    #[test]
    fn a_settled_answer_settles_on_either_run() {
        for repriced in [false, true] {
            assert!(matches!(next_step(settled(), repriced), Step::Settle(_)));
        }
    }

    #[test]
    fn an_answer_settled_before_writes_nothing_on_either_run() {
        for repriced in [false, true] {
            assert!(matches!(
                next_step(ReportAnswer::AlreadySettled, repriced),
                Step::AlreadySettled
            ));
        }
    }

    #[test]
    fn a_first_reprice_reruns_under_the_leases_own_facts() {
        let Step::Reprice(facts) = next_step(ReportAnswer::Repriced(held()), false) else {
            unreachable!("a first miss reruns");
        };
        assert_eq!(facts, held());
    }

    #[test]
    fn a_second_reprice_stops_at_the_known_lines() {
        assert!(matches!(
            next_step(ReportAnswer::Repriced(held()), true),
            Step::KnownRequestsOnly
        ));
    }

    #[test]
    fn a_moved_fence_stops_at_the_known_lines_on_either_run() {
        for repriced in [false, true] {
            assert!(matches!(
                next_step(ReportAnswer::Fenced, repriced),
                Step::KnownRequestsOnly
            ));
        }
    }
}
