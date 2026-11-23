//! A silent runner, recorded once, at the instant its connection's end names.
//!
//! Nothing tells this daemon that a runner died, but the gateway sees its
//! connection end. That end reads when the runner last beat and, at that beat
//! plus the offline window, runs one transaction for that one runner: recorded
//! `runner_offline` once for the episode, cordoned for missed heartbeats with
//! its operator status left alone, and a requested drain finished when no live
//! run remains. A beat after the connection ended moved `last_seen_at`, and the
//! transaction then writes nothing.

use std::time::Duration;

use af_core::id::Uuid7;
use afd_state::sql::{RUNNER_STATUS_DRAINED, RUNNER_STATUS_DRAINING};
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use serde_json::{Value, json};
use sqlx::{Acquire as _, PgConnection, types::Json};

use crate::error::{Result, query};
use crate::{sql, store::Runners};

/// Statement names, for the context a query failure carries.
const CONTEXT_LAST_SEEN: &str = "runner last beat read";
const CONTEXT_OFFLINE: &str = "liveness offline visit";

impl Runners {
    /// Silence after which a runner is recorded offline.
    #[must_use]
    pub const fn offline_after(&self) -> Duration {
        self.clock().offline_after()
    }

    /// When `runner` last beat; `None` when it never has, or is gone.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn last_seen(&self, runner: &Uuid7) -> Result<Option<Timestamp>> {
        let mut connection = self.pool().acquire().await?;
        let seen: Option<jiff_sqlx::Timestamp> =
            sqlx::query_scalar(sql::runner::SELECT_RUNNER_LAST_SEEN)
                .bind(runner)
                .bind(sql::NEVER_SEEN.to_sqlx())
                .fetch_optional(&mut *connection)
                .await
                .map_err(query(CONTEXT_LAST_SEEN))?;
        Ok(seen.map(jiff_sqlx::Timestamp::to_jiff))
    }

    /// Visits `runner` once its offline window has passed since `observed`,
    /// the beat its connection's end read. `now` mints the history rows'
    /// identifiers; the database stamps them.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a statement Postgres
    /// refused, and an identifier that would not mint.
    pub async fn record_silence(
        &self,
        runner: &Uuid7,
        observed: Timestamp,
        now: Timestamp,
    ) -> Result<()> {
        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_OFFLINE))?;
        let Some(cordoned) = sqlx::query_scalar::<_, bool>(sql::runner::MARK_RUNNER_SILENT)
            .bind(runner)
            .bind(observed.to_sqlx())
            .bind(sql::NEVER_SEEN.to_sqlx())
            .bind(sql::RUNNER_STATUS_ACTIVE)
            .bind(crate::cordon::REASON)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT_OFFLINE))?
        else {
            // It beat again, or is gone: the dropped transaction wrote nothing.
            return Ok(());
        };
        sqlx::query(sql::runner::INSERT_OFFLINE_HISTORY)
            .bind(self.entropy().uuid7(now)?)
            .bind(runner)
            .bind(sql::event_type::RUNNER_OFFLINE)
            .bind(Json(
                json!({ sql::meta::LAST_SEEN_AT: observed.to_string() }),
            ))
            .bind(observed.to_sqlx())
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_OFFLINE))?;
        if cordoned {
            let metadata = json!({ sql::meta::REASON: crate::cordon::REASON });
            self.history(
                &mut transaction,
                runner,
                crate::cordon::CORDONED,
                metadata,
                now,
            )
            .await?;
        }
        let drained: Option<Uuid7> = sqlx::query_scalar(sql::runner::FINISH_DRAIN)
            .bind(runner)
            .bind(RUNNER_STATUS_DRAINING)
            .bind(RUNNER_STATUS_DRAINED)
            .bind(sql::RUN_STATUS_ACTIVE)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT_OFFLINE))?;
        if drained.is_some() {
            let metadata = json!({
                sql::meta::FROM_STATUS: RUNNER_STATUS_DRAINING,
                sql::meta::TO_STATUS: RUNNER_STATUS_DRAINED,
            });
            self.history(
                &mut transaction,
                runner,
                sql::event_type::RUNNER_DRAINED,
                metadata,
                now,
            )
            .await?;
        }
        transaction.commit().await.map_err(query(CONTEXT_OFFLINE))?;
        if cordoned {
            af_observability::producers::fleet::runner::cordon_changed(
                af_observability::metrics::label::fleet::CordonChange::Cordoned,
            );
            // Hoisted out of the macro: `tracing`'s `log` feature compiles an
            // event's fields twice.
            let (event, runner_id, reason) = (
                crate::cordon::CORDONED,
                runner.as_str(),
                crate::cordon::REASON,
            );
            tracing::info!(event, runner_id, reason);
        }
        Ok(())
    }

    async fn history(
        &self,
        connection: &mut PgConnection,
        runner: &Uuid7,
        event_type: &str,
        metadata: Value,
        now: Timestamp,
    ) -> Result<()> {
        sqlx::query(sql::runner::INSERT_HISTORY)
            .bind(self.entropy().uuid7(now)?)
            .bind(runner)
            .bind(event_type)
            .bind(Json(metadata))
            .execute(connection)
            .await
            .map_err(query(CONTEXT_OFFLINE))?;
        Ok(())
    }
}
