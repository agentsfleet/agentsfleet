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
//! Five error handlers that each log and carry on would leave "this write is
//! best-effort" a property a reader reconstructs from the shape of five
//! bodies. Here it is [`best_effort`] — one function, named, with the reason in
//! its documentation, and a call site that reads as a decision rather than as
//! an omission.

use af_core::error_code::{self, Coded as _, Logged};
use af_core::id::Uuid7;
use af_core::timing::RUNNER_OFFLINE_AFTER;
use af_observability::producers;
use af_wire::runner::{CapabilityReport, HeartbeatRequest, HeldFleets, SelftestReport};
use garde::{Valid, Validate as _};
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::{Executor as _, PgConnection};

use crate::bounds;
use crate::error::{Error, Result, runner_vanished};
use crate::policy::{AssignmentColumns, capability};
use crate::reconcile::{Verdict, reconcile};
use crate::spelling::render_list;
use crate::sql;
use crate::store::Runners;
use afd_db::statement::Bound;

/// The scoped events a best-effort write reports itself under.
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
    holds: HeldFleets(Vec::new()),
    closing: false,
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
    /// Fleets the beat listed as held that the runner should let go of.
    pub release_holds: Vec<String>,
}

use crate::capacity::authority::{Authority, Decision};
use policy::PolicyRow;

impl Runners {
    /// Records a beat and answers what the host must apply. `held` is the
    /// beat's holds list as its handler proved it; `None` holds nothing. A
    /// beat whose `closing` is set clears every hold the list leaves out at
    /// once, since a closing runner's list is final. `now` mints the
    /// transition event's identifier and dates the liveness gauge; every row
    /// this beat writes is stamped by the database.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a statement Postgres refused,
    /// a stored assignment this build cannot read, and — as its own kind — a
    /// token that authenticated against a row which has since been reaped.
    /// Nothing else fails a beat: see the module documentation for which writes
    /// are best-effort and why.
    pub async fn heartbeat(
        &self,
        runner: &Uuid7,
        beat: &HeartbeatRequest<'_>,
        held: Option<&Valid<HeldFleets<'_>>>,
        now: Timestamp,
    ) -> Result<Beat> {
        let mut connection = self.pool().acquire().await?;
        let row = match policy::read(&mut connection, runner, None).await? {
            Decision::Allowed(row) => row,
            Decision::Refused(_) => return Err(runner_vanished()),
        };
        self.record_beat(&mut connection, runner, beat, held, now, row)
            .await
    }

    /// Record liveness only after the same policy read proves this physical owner.
    ///
    /// # Errors
    /// Preserves database failures; changed credentials and owners are explicit refusals.
    pub async fn heartbeat_connected(
        &self,
        authority: &Authority,
        beat: &HeartbeatRequest<'_>,
        held: Option<&Valid<HeldFleets<'_>>>,
        now: Timestamp,
    ) -> Result<Decision<Beat>> {
        let mut connection = self.pool().acquire().await?;
        let row = match policy::read(&mut connection, authority.runner(), Some(authority)).await? {
            Decision::Allowed(row) => row,
            Decision::Refused(refusal) => return Ok(Decision::Refused(refusal)),
        };
        Ok(Decision::Allowed(
            self.record_beat(&mut connection, authority.runner(), beat, held, now, row)
                .await?,
        ))
    }

    /// Recheck ownership and policy without pretending that a silent host sent a heartbeat.
    ///
    /// # Errors
    /// Preserves database failures; callers keep their previous confirmed deadlines.
    pub async fn recheck_connection(&self, authority: &Authority) -> Result<Decision<Beat>> {
        let row = match policy::read(
            &mut *self.pool().acquire().await?,
            authority.runner(),
            Some(authority),
        )
        .await?
        {
            Decision::Allowed(row) => row,
            Decision::Refused(refusal) => return Ok(Decision::Refused(refusal)),
        };
        let report = capability(row.capability_report_json.as_deref());
        let assigned = row.assignment.decode()?;
        let verdict = reconcile(&assigned, report.as_ref());
        Ok(Decision::Allowed(Beat {
            assignment: row.assignment,
            verdict,
            selftest_requested: row.selftest_requested,
            release_holds: Vec::new(),
        }))
    }

    async fn record_beat(
        &self,
        connection: &mut PgConnection,
        runner: &Uuid7,
        beat: &HeartbeatRequest<'_>,
        held: Option<&Valid<HeldFleets<'_>>>,
        now: Timestamp,
        row: PolicyRow,
    ) -> Result<Beat> {
        let incoming = bounded_report(beat);
        let stored = capability(row.capability_report_json.as_deref());
        let assigned = row.assignment.decode()?;
        let verdict = reconcile(&assigned, incoming.or(stored.as_ref()));

        verdict::persist(connection, runner, incoming, &row.stored, verdict).await;
        // After the capability write, so a malformed verdict cannot cost the
        // beat its reconciliation.
        let reported = persist_selftest(connection, runner, beat.selftest.as_ref()).await;
        self.bump_liveness(connection, runner, now).await;
        let release_holds = holds::reconcile(connection, runner, held, beat.closing).await;
        // The gauge's only input. Liveness is a Postgres row a collection
        // callback cannot read — it is a network round trip, and the SDK
        // collects on a thread that must not make one — so the beat that
        // writes the row publishes the same instant for the gauge to serve.
        producers::fleet::runner::seen(runner.as_str(), now);

        Ok(Beat {
            selftest_requested: row.selftest_requested && !reported,
            release_holds,
            assignment: row.assignment,
            verdict,
        })
    }

    /// Bumps liveness, emitting a transition event only on a real transition.
    ///
    /// Falls back to the liveness-only statement when the event identifier
    /// cannot be minted or the combined statement is refused: a beat that
    /// writes liveness without its audit row is a quieter loss than one that
    /// writes nothing, because the fleet read stays true.
    async fn bump_liveness(&self, connection: &mut PgConnection, runner: &Uuid7, now: Timestamp) {
        let landed = match self.event_id(now) {
            Ok(event_id) => {
                let with_event = sqlx::query(sql::runner::HEARTBEAT_WITH_TRANSITION_EVENT)
                    .bind(runner)
                    .bind(event_id)
                    .bind(sql::event_type::RUNNER_ONLINE)
                    .bind(sql::meta::LAST_SEEN_AT)
                    .bind(sql::NEVER_SEEN.to_sqlx())
                    .bind(RUNNER_OFFLINE_AFTER);
                best_effort(with_event, connection, EVENT_LIVENESS_WRITE, runner).await
            }
            Err(error) => {
                report(EVENT_LIVENESS_WRITE, runner, &error);
                false
            }
        };
        if !landed {
            let touch = sqlx::query(sql::runner::TOUCH_RUNNER_LAST_SEEN).bind(runner);
            best_effort(touch, connection, EVENT_LIVENESS_WRITE, runner).await;
        }
    }

    /// Draws the identifier a transition event is written under.
    fn event_id(&self, now: Timestamp) -> Result<Uuid7> {
        Ok(self.entropy().uuid7(now)?)
    }
}

/// Stores a reported verdict, and answers whether anything was written.
///
/// Silent on a malformed verdict, exactly like the capability report: a runner
/// token must not be able to fail a liveness beat by sending nonsense.
async fn persist_selftest(
    connection: &mut PgConnection,
    runner: &Uuid7,
    reported: Option<&SelftestReport<'_>>,
) -> bool {
    let Some(report) = reported else {
        return false;
    };
    if let Err(rejection) = bounds::accept(report) {
        refuse_selftest(runner, &rejection);
        return false;
    }

    let checks_json = render_list(&report.checks);
    let write = sqlx::query(sql::runner::UPDATE_RUNNER_SELFTEST)
        .bind(runner)
        .bind(checks_json)
        .bind(report.sandbox_tier.as_ref())
        .bind(report.network_policy.as_ref());
    best_effort(write, connection, EVENT_SELFTEST_WRITE, runner).await
}

/// Logs a verdict the bounds refused; the beat still counts as liveness.
///
/// The refusal's own sentence, never its rendering: the code is the line's
/// `error_code` field already.
fn refuse_selftest(runner: &Uuid7, rejection: &Error) {
    let code = error_code::INVALID_REQUEST.as_str();
    let id = runner.as_str();
    let reason = rejection.detail();
    tracing::warn!(
        error_code = code,
        runner_id = id,
        reason,
        event = "selftest_verdict_refused",
        "self-test verdict refused; the beat still counts as liveness"
    );
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

/// The capability report a beat carries, when it is within the bounds its
/// wire type declares.
///
/// A report past them is not a report: the same lenient answer as an
/// unreadable body, so the stored one keeps reconciling and the beat lands.
fn bounded_report<'b, 'a>(beat: &'b HeartbeatRequest<'a>) -> Option<&'b CapabilityReport<'a>> {
    beat.capability_report
        .as_ref()
        .filter(|report| report.validate().is_ok())
}

/// Reports a best-effort step that could not even be attempted.
fn report(event: &'static str, runner: &Uuid7, error: &Error) {
    let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let id = runner.as_str();
    let Logged {
        error_code: cause_error_code,
        reason,
    } = error.logged();
    tracing::warn!(
        error_code = code,
        cause_error_code,
        runner_id = id,
        reason,
        event
    );
}

pub mod holds;
mod policy;
mod verdict;

#[cfg(test)]
mod tests;
