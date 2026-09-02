//! The beat: liveness up, assignment down, and the verdict reconciled between.
//!
//! One request, four writes, and only ONE of them may fail the reply. The
//! policy read is load-bearing — the reply IS the delivery channel for an
//! operator's assignment, so a host that gets a 200 with no policy would apply
//! nothing. Every write after it is best-effort: a verdict that does not land
//! is re-derived next beat, a self-test that does not land is re-run on ask,
//! and a liveness bump that does not land costs one interval of freshness. A
//! beat that failed on any of those would park real work over a blip.
//!
//! # Why the split is a function and not a comment
//!
//! `heartbeat.zig` makes the same choice and spells it as five `catch` blocks
//! that each log and carry on. Nothing relates them, so "this write is
//! best-effort" is a property a reader reconstructs from the shape of five
//! bodies. Here it is [`best_effort`] — one function, named, with the reason in
//! its documentation, and a call site that reads as a decision rather than as
//! an omission.

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_core::timing::RUNNER_OFFLINE_AFTER_MS;
use afd_observability::producers;
use afd_wire::runner::{CapabilityReport, HeartbeatRequest, SelftestReport};
use sqlx::{Executor as _, PgConnection, Row as _};

use crate::bounds;
use crate::error::{Error, Result, query};
use crate::policy::{AssignmentColumns, StoredVerdict, capability};
use crate::reconcile::{Verdict, reconcile};
use crate::spelling::render_list;
use crate::sql;
use crate::sql::runner::Bound;
use crate::store::Runners;

/// The statement name a policy-read failure carries.
const CONTEXT_POLICY_READ: &str = "runner policy read";

/// The scoped events a best-effort write reports itself under.
const EVENT_VERDICT_WRITE: &str = "verdict_persist_failed";
const EVENT_SELFTEST_WRITE: &str = "selftest_persist_failed";
const EVENT_LIVENESS_WRITE: &str = "heartbeat_bump_failed";

/// A beat that carried nothing — the shape a body-less or unreadable heartbeat
/// resolves to.
///
/// A runner token must not be able to fail a liveness beat by sending nonsense,
/// so the caller substitutes this rather than refusing the request; the stored
/// report keeps reconciling and the host is told its liveness landed.
pub const NO_REPORT: HeartbeatRequest<'static> = HeartbeatRequest {
    capability_report: None,
    selftest: None,
};

/// What one beat resolved to, for the reply the host reads.
#[derive(Debug)]
pub struct Beat {
    /// The assignment as stored, for the caller to decode and echo.
    pub assignment: AssignmentColumns,
    /// The verdict this beat reconciled.
    pub verdict: Verdict,
    /// Whether an operator's self-test request is still outstanding.
    ///
    /// Suppressed on the beat that just reported one: the write cleared the
    /// ask, so echoing it back would tell the host to immediately re-run the
    /// probe it has this second finished.
    pub selftest_requested: bool,
}

/// The policy row, as the reconciliation needs it.
#[derive(Debug)]
struct PolicyRow {
    assignment: AssignmentColumns,
    stored: StoredVerdict,
    capability_report_json: Option<String>,
    selftest_requested: bool,
}

impl Runners {
    /// Records a beat and answers what the host must apply.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a statement Postgres refused,
    /// and — as its own kind — a token that authenticated against a row which
    /// has since been reaped. Nothing else fails a beat: see the module
    /// documentation for which writes are best-effort and why.
    pub async fn heartbeat(
        &self,
        runner: &Uuid7,
        beat: &HeartbeatRequest<'_>,
        now: UnixMillis,
    ) -> Result<Beat> {
        let mut connection = self.pool().acquire().await?;
        let row = self.policy_row(&mut connection, runner).await?;

        // A report past its bounds is not a report. Same lenient answer as an
        // unreadable body: the stored one keeps reconciling.
        let incoming = beat
            .capability_report
            .as_ref()
            .filter(|report| bounds::capability_within_bounds(report));
        let stored = capability(row.capability_report_json.as_deref());
        let assigned = row.assignment.decode();
        let verdict = reconcile(assigned.as_ref(), incoming.or(stored.as_ref()));

        persist_verdict(&mut connection, runner, incoming, &row.stored, verdict, now).await;
        // After the capability write, so a malformed verdict cannot cost the
        // beat its reconciliation.
        let reported = persist_selftest(&mut connection, runner, beat.selftest.as_ref(), now).await;
        self.bump_liveness(&mut connection, runner, now).await;
        // The gauge's only input. Liveness is a Postgres row a collection
        // callback cannot read — it is a network round trip, and the SDK
        // collects on a thread that must not make one — so the beat that
        // writes the row publishes the same instant for the gauge to serve.
        producers::fleet::runner::seen(runner.as_str(), now.as_millis());

        Ok(Beat {
            selftest_requested: row.selftest_requested && !reported,
            assignment: row.assignment,
            verdict,
        })
    }

    /// Reads the assignment, the stored report, and the prior verdict.
    ///
    /// The one read that fails loud: the reply is how an assignment reaches the
    /// host, so answering 200 with no policy would tell a runner to apply
    /// nothing.
    async fn policy_row(&self, connection: &mut PgConnection, runner: &Uuid7) -> Result<PolicyRow> {
        let found = sqlx::query(sql::runner::SELECT_RUNNER_ASSIGNED_POLICY)
            .bind(runner.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_POLICY_READ))?;
        // Fail closed rather than beat a phantom runner: the token is real and
        // the enrolment is gone, so the host must be re-enrolled.
        let row = found.ok_or_else(|| Error::RunnerVanished)?;

        let column = query(CONTEXT_POLICY_READ);
        Ok(PolicyRow {
            assignment: AssignmentColumns {
                sandbox_tier: row.try_get(0).map_err(&column)?,
                network_policy: row.try_get(1).map_err(&column)?,
                registry_allowlist_json: row.try_get(2).map_err(&column)?,
                worker_count: row.try_get(3).map_err(&column)?,
                extra_binds_json: row.try_get(8).map_err(&column)?,
            },
            stored: StoredVerdict {
                degraded: row.try_get(4).map_err(&column)?,
                reason: row.try_get(5).map_err(&column)?,
            },
            capability_report_json: row.try_get(6).map_err(&column)?,
            // A decided boolean rather than the raw instant: the reply carries
            // "is one pending", and nothing downstream has a use for when it
            // was made.
            selftest_requested: row.try_get::<Option<i64>, _>(7).map_err(&column)?.is_some(),
        })
    }

    /// Bumps liveness, emitting a transition event only on a real transition.
    ///
    /// Falls back to the liveness-only statement when the event identifier
    /// cannot be minted or the combined statement is refused: a beat that
    /// writes liveness without its audit row is a quieter loss than one that
    /// writes nothing, because the fleet read stays true.
    async fn bump_liveness(&self, connection: &mut PgConnection, runner: &Uuid7, now: UnixMillis) {
        let landed = match self.event_id(now) {
            Ok(event_id) => {
                let millis = now.as_millis();
                let with_event = sqlx::query(sql::runner::HEARTBEAT_WITH_TRANSITION_EVENT)
                    .bind(runner.as_str())
                    .bind(millis)
                    .bind(event_id.as_str())
                    .bind(sql::event_type::RUNNER_ONLINE)
                    .bind(sql::meta::LAST_SEEN_AT)
                    .bind(sql::LAST_SEEN_NEVER)
                    .bind(RUNNER_OFFLINE_AFTER_MS);
                best_effort(with_event, connection, EVENT_LIVENESS_WRITE, runner).await
            }
            Err(error) => {
                report(EVENT_LIVENESS_WRITE, runner, &error);
                false
            }
        };
        if !landed {
            let touch = sqlx::query(sql::runner::TOUCH_RUNNER_LAST_SEEN)
                .bind(runner.as_str())
                .bind(now.as_millis());
            best_effort(touch, connection, EVENT_LIVENESS_WRITE, runner).await;
        }
    }

    /// Draws the identifier a transition event is written under.
    fn event_id(&self, now: UnixMillis) -> Result<Uuid7> {
        let mut bytes = [0u8; ENTROPY_LEN];
        self.entropy().fill(&mut bytes)?;
        Ok(Uuid7::encode(now, bytes)?)
    }
}

/// Writes what this beat changed about the verdict.
///
/// A fresh report always lands with its verdict; otherwise only a MOVED verdict
/// writes, and the statement's own guard makes a steady state write nothing at
/// all. The `differs_from` check ahead of it saves the round trip that guard
/// would otherwise cost on every beat of every idle host.
async fn persist_verdict(
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

/// Stores a reported verdict, and answers whether anything was written.
///
/// Silent on a malformed verdict, exactly like the capability report: a runner
/// token must not be able to fail a liveness beat by sending nonsense.
async fn persist_selftest(
    connection: &mut PgConnection,
    runner: &Uuid7,
    reported: Option<&SelftestReport<'_>>,
    now: UnixMillis,
) -> bool {
    let Some(report) = reported else {
        return false;
    };
    if let Err(rejection) = bounds::accept(report) {
        let code = error_code::INVALID_REQUEST.as_str();
        let id = runner.as_str();
        let reason = rejection.to_string();
        tracing::warn!(
            error_code = code,
            runner_id = id,
            reason,
            event = "selftest_verdict_refused",
            "self-test verdict refused; the beat still counts as liveness"
        );
        return false;
    }

    let checks_json = render_list(&report.checks);
    let write = sqlx::query(sql::runner::UPDATE_RUNNER_SELFTEST)
        .bind(runner.as_str())
        .bind(checks_json)
        .bind(report.all_ok)
        .bind(report.sandbox_tier.as_ref())
        .bind(report.network_policy.as_ref())
        .bind(now.as_millis());
    best_effort(write, connection, EVENT_SELFTEST_WRITE, runner).await
}

/// Runs a write whose failure must not fail the beat, and says whether it
/// landed.
///
/// The single place the best-effort rule is expressed. Every caller of it is a
/// write the next beat re-derives, so a failure is logged at `warn` — visible
/// to an operator watching one host, invisible to the reply.
async fn best_effort(
    statement: Bound<'_>,
    connection: &mut PgConnection,
    event: &'static str,
    runner: &Uuid7,
) -> bool {
    match connection.execute(statement).await {
        Ok(_landed) => true,
        Err(error) => {
            let code = error_code::INTERNAL_DB_QUERY.as_str();
            let id = runner.as_str();
            let reason = error.to_string();
            tracing::warn!(error_code = code, runner_id = id, reason, event);
            false
        }
    }
}

/// Reports a best-effort step that could not even be attempted.
fn report(event: &'static str, runner: &Uuid7, error: &Error) {
    let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let id = runner.as_str();
    let reason = error.to_string();
    tracing::warn!(error_code = code, runner_id = id, reason, event);
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
