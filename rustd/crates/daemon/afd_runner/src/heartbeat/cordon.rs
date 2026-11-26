//! Mutation-time connection proof and uninterrupted healthy return.

use af_core::id::Uuid7;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::PgConnection;

use af_wire::runner::{CapabilityReport, HeartbeatRequest};

use super::{EVENT_LIVENESS_WRITE, host::HostColumns, report};
use crate::bounds;
use crate::reconcile::Verdict;
use crate::{capacity::authority::Authority, sql, store::Runners};

pub(super) struct Health<'a> {
    pub(super) authority: Option<&'a Authority>,
    pub(super) valid: bool,
    pub(super) policy: &'a serde_json::Value,
    /// The host facts the beat proved inside their wire bounds.
    pub(super) host: HostColumns,
}

impl Runners {
    /// None preserves best-effort heartbeat semantics on a datastore failure;
    /// false means the statement proved this stream no longer owns the host.
    pub(super) async fn bump_liveness(
        &self,
        connection: &mut PgConnection,
        runner: &Uuid7,
        now: Timestamp,
        health: Health<'_>,
    ) -> Option<bool> {
        match (self.event_id(now), self.event_id(now)) {
            (Ok(online), Ok(lift)) => {
                let result: Result<(bool, bool), sqlx::Error> =
                    sqlx::query_as(sql::runner::HEARTBEAT_WITH_TRANSITION_HISTORY)
                        .bind(runner)
                        .bind(online)
                        .bind(sql::event_type::RUNNER_ONLINE)
                        .bind(sql::meta::LAST_SEEN_AT)
                        .bind(sql::NEVER_SEEN.to_sqlx())
                        .bind(self.clock().offline_after())
                        .bind(health.authority.map(|owner| owner.digest().as_str()))
                        .bind(health.authority.map(Authority::connection))
                        .bind(health.valid)
                        .bind(self.clock().heartbeat_interval().saturating_mul(2))
                        .bind(self.clock().uncordon_after())
                        .bind(lift)
                        .bind(crate::cordon::LIFTED)
                        .bind(sql::meta::REASON)
                        .bind(crate::cordon::REASON)
                        .bind(sql::RUNNER_STATUS_ACTIVE)
                        .bind(sqlx::types::Json(health.policy))
                        .bind(health.host.cpus)
                        .bind(health.host.memory)
                        .bind(health.host.free_disk)
                        .fetch_one(&mut *connection)
                        .await;
                match result {
                    Ok((accepted, lifted)) => {
                        if lifted {
                            announce_lift(runner);
                        }
                        return Some(accepted);
                    }
                    Err(error) => {
                        write_failed(runner, &error);
                    }
                }
            }
            (Err(error), _) | (_, Err(error)) => report(EVENT_LIVENESS_WRITE, runner, &error),
        }
        let result: Result<Option<Uuid7>, sqlx::Error> =
            sqlx::query_scalar(sql::runner::TOUCH_RUNNER_LAST_SEEN)
                .bind(runner)
                .bind(health.authority.map(|owner| owner.digest().as_str()))
                .bind(health.authority.map(Authority::connection))
                .bind(sql::RUNNER_STATUS_ACTIVE)
                .fetch_optional(connection)
                .await;
        match result {
            Ok(row) => Some(row.is_some()),
            Err(error) => {
                write_failed(runner, &error);
                None
            }
        }
    }
}

/// Whether the beat itself reads healthy: not degraded, not closing, and
/// carrying no report or verdict the bounds refuse.
pub(super) fn healthy(
    beat: &HeartbeatRequest<'_>,
    incoming: Option<&CapabilityReport<'_>>,
    verdict: Verdict,
) -> bool {
    !verdict.is_degraded()
        && !beat.closing
        && (beat.capability_report.is_none() || incoming.is_some())
        && beat
            .selftest
            .as_ref()
            .is_none_or(|report| bounds::accept(report).is_ok())
}

fn write_failed(runner: &Uuid7, error: &sqlx::Error) {
    let error_code = af_core::error_code::INTERNAL_DB_QUERY.as_str();
    tracing::warn!(event = EVENT_LIVENESS_WRITE, runner_id = runner.as_str(), error_code, reason = %error);
}

/// Counts and logs a cordon a healthy return lifted.
fn announce_lift(runner: &Uuid7) {
    af_observability::producers::fleet::runner::cordon_changed(
        af_observability::metrics::label::fleet::CordonChange::Lifted,
    );
    // Hoisted out of the macro: `tracing`'s `log`
    // feature compiles an event's fields twice.
    let (event, runner_id, reason) = (
        "runner_cordon_lifted",
        runner.as_str(),
        crate::cordon::REASON,
    );
    tracing::info!(event, runner_id, reason);
}
