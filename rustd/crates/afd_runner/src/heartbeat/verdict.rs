//! What a beat changed about a runner's verdict: the write, and the line an
//! operator reads when it moved.

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_wire::runner::CapabilityReport;
use sqlx::PgConnection;

use super::best_effort;
use crate::policy::StoredVerdict;
use crate::reconcile::Verdict;
use crate::sql;

/// The scoped event a verdict write that did not land is logged under.
const EVENT_VERDICT_WRITE: &str = "verdict_persist_failed";

/// Writes what this beat changed about the verdict.
///
/// A fresh report always lands with its verdict; otherwise only a MOVED verdict
/// writes, and the statement's own guard makes a steady state write nothing at
/// all. The `differs_from` check ahead of it saves the round trip that guard
/// would otherwise cost on every beat of every idle host.
pub(super) async fn persist(
    connection: &mut PgConnection,
    runner: &Uuid7,
    incoming: Option<&CapabilityReport<'_>>,
    stored: &StoredVerdict,
    verdict: Verdict,
    now: UnixMillis,
) {
    let millis = now.as_millis();
    if let Some(report) = incoming {
        let report_json = serde_json::to_string(report).unwrap_or_else(|_unreachable| {
            // Unreachable for this shape — booleans and a string list — and an
            // empty object is the honest degradation: it stores "reported
            // nothing" rather than a half-written report.
            "{}".to_owned()
        });
        let write = sqlx::query(sql::runner::UPDATE_RUNNER_CAPABILITY_AND_VERDICT)
            .bind(runner.as_str())
            .bind(report_json)
            .bind(millis)
            .bind(verdict.is_degraded())
            .bind(verdict.reason());
        best_effort(write, connection, EVENT_VERDICT_WRITE, runner).await;
    } else if stored.differs_from(verdict) {
        let write = sqlx::query(sql::runner::UPDATE_RUNNER_VERDICT)
            .bind(runner.as_str())
            .bind(verdict.is_degraded())
            .bind(verdict.reason())
            .bind(millis);
        best_effort(write, connection, EVENT_VERDICT_WRITE, runner).await;
    }
    announce(runner, stored, verdict);
}

/// Says a verdict CHANGED, and only when it changed.
///
/// A degradation is an operator event — a host that will not take work — so it
/// is `warn` on the transition and silent on every beat after it. Logging the
/// state rather than the transition would put one line per host per ten seconds
/// into the log and hide the moment it happened.
fn announce(runner: &Uuid7, stored: &StoredVerdict, verdict: Verdict) {
    let id = runner.as_str();
    match (stored.degraded, verdict.reason()) {
        (false, Some(reason)) => {
            let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
            tracing::warn!(
                error_code = code,
                runner_id = id,
                reason,
                event = "runner_degraded",
                "runner degraded — it will not be assigned work until this is fixed"
            );
        }
        (true, None) => tracing::debug!(runner_id = id, event = "runner_recovered"),
        _steady => {}
    }
}
