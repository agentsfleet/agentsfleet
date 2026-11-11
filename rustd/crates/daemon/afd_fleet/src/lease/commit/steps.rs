//! The steps a terminal report takes around its one statement: what to do with
//! the statement's answer, how a statement that wrote nothing lets go of its
//! connection, and where a task's notice and an unknown reply connector land.

use af_core::id::Uuid7;
use sqlx::PgConnection;

use super::{CONTEXT_COMMIT, TerminalReport};
use crate::error::{Result, query};
use crate::lease::settle::Reported;
use crate::lease::sql::report_statement::{HeldFacts, ReportAnswer, SettledReport};

/// Logged when a stored reply connector names no connector this build knows.
const EVENT_REPLY_PROVIDER_UNKNOWN: &str = "report_reply_provider_unknown";

/// What a report does after one run of its statement.
#[derive(Debug)]
pub(super) enum Step {
    /// Every write landed; the task and folds follow in the transaction.
    Settle(Box<SettledReport>),
    /// This runner settled the lease before.
    AlreadySettled,
    /// The lease holds pricing facts other than the ones bound: price again
    /// under these and rerun, once.
    Reprice(HeldFacts),
    /// The fence moved, or the catalogue moved again under the rerun: only the
    /// runner's known request lines are logged, and the lease is
    /// left to its own renewal or expiry.
    KnownRequestsOnly,
}

/// The step after `answer`, when `repriced` says whether this is the rerun.
///
/// A catalogue edited between the price read and the statement costs one
/// rerun, for every lease (the spec's failure table). A second miss means a
/// second edit inside one report; settling under a third read would not be
/// more right, so the report stops at its known lines.
pub(super) fn next_step(answer: ReportAnswer, repriced: bool) -> Step {
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
pub(super) async fn release(transaction: sqlx::Transaction<'_, sqlx::Postgres>) -> Result<()> {
    transaction.rollback().await.map_err(query(CONTEXT_COMMIT))
}

/// Puts a task's notice where the run's answer was: the event, the session
/// and the answer owed, which a run that answered nothing did not owe yet.
pub(super) async fn apply_notice(
    connection: &mut PgConnection,
    report: &TerminalReport<'_>,
    notice: &str,
    (obligation_id, receipt): &(Uuid7, Uuid7),
) -> Result<()> {
    let checkpoint = crate::lease::finalize::checkpoint_document(report.last_event_id, notice);
    let _events: i64 = sqlx::query_scalar(crate::lease::sql::report_statement::APPLY_REPORT_NOTICE)
        .bind(&report.lease.fleet_id)
        .bind(&report.lease.event_id)
        .bind(notice)
        .bind(&checkpoint)
        .bind(obligation_id)
        .bind(receipt)
        .bind(
            afd_connector::Provider::ALL
                .iter()
                .map(|provider| provider.id())
                .collect::<Vec<_>>(),
        )
        .fetch_one(connection)
        .await
        .map_err(query(CONTEXT_COMMIT))?;
    Ok(())
}

/// Logs a stored reply connector no connector of this build names: the
/// answer is owed nowhere, as the read used to log.
pub(super) fn warn_unknown_reply(lease: &Reported, stored: Option<&str>) {
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
