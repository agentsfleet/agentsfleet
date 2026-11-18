//! Mutation-time connection proof and uninterrupted healthy return.

use af_core::id::Uuid7;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::PgConnection;

use super::{EVENT_LIVENESS_WRITE, report};
use crate::{capacity::authority::Authority, sql, store::Runners};

pub(super) struct Health<'a> {
    pub(super) authority: Option<&'a Authority>,
    pub(super) valid: bool,
    pub(super) policy: &'a serde_json::Value,
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
                    sqlx::query_as(sql::runner::HEARTBEAT_WITH_TRANSITION_EVENT)
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
                        .bind("reason")
                        .bind(crate::cordon::REASON)
                        .bind(sql::RUNNER_STATUS_ACTIVE)
                        .bind(sqlx::types::Json(health.policy))
                        .fetch_one(&mut *connection)
                        .await;
                match result {
                    Ok((accepted, lifted)) => {
                        if lifted {
                            af_observability::producers::fleet::runner::cordon_changed(
                                af_observability::metrics::label::fleet::CordonChange::Lifted,
                            );
                            tracing::info!(
                                event = "runner_cordon_lifted",
                                runner_id = runner.as_str(),
                                reason = crate::cordon::REASON
                            );
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

fn write_failed(runner: &Uuid7, error: &sqlx::Error) {
    let error_code = af_core::error_code::INTERNAL_DB_QUERY.as_str();
    tracing::warn!(event = EVENT_LIVENESS_WRITE, runner_id = runner.as_str(), error_code, reason = %error);
}
