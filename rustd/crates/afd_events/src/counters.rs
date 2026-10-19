//! A fleet's activity counters, read right before a frame carries them.
//!
//! Every frame the daemon publishes on a fleet's tail carries an absolute
//! counter snapshot (`afd_api_wire::tail::FleetCounters`), and the publishers
//! live in three crates. The statement and its decoder live here, once, so a
//! column added to the counters table reaches every publisher or none.
//!
//! # Best-effort, because the publish is
//!
//! A frame that does not land costs the tail a marker and the run nothing;
//! a counter that could not be read costs the frame its figures and the run
//! nothing. [`fleet_counters`] answers the crate's error for a caller that
//! wants it. [`fleet_counters_best_effort`] is for the publishers: it logs
//! the failure with its registry code and answers `None`, so the frame goes
//! out with the counters absent — which the client reads as "leave what you
//! have standing" — rather than with zeros, which it would read as a fleet
//! that has done nothing.
//!
//! # On the connection the write held, where there is one
//!
//! A publisher whose own write moved the counters — the receive, the
//! continuation, the park — still holds that connection when the read is
//! due, and the trigger's write is visible to the next statement on it. The
//! `_on` variants read there, so the hot path pays one statement and no
//! second acquire; the pool-taking variants are for the publishers whose
//! write already returned its connection.

use afd_api_wire::tail::FleetCounters;
use afd_core::error_code::{Coded as _, Logged};
use afd_db::Db;
use sqlx::PgConnection;

use crate::error::{Result, query};
use crate::sql::SELECT_FLEET_COUNTERS;

/// The context a refused counter read is reported under.
const CONTEXT_COUNTERS: &str = "fleet activity counters";

/// The counters as `fleet.activity_counters` spells them.
#[derive(sqlx::FromRow)]
struct StoredCounters {
    event_count: i64,
    budget_used_nanos: i64,
}

/// The log event a publisher emits when it goes out without its figures.
const EVENT_COUNTERS_UNREAD: &str = "fleet_counters_unread";

/// The fleet's counters as the database has them.
///
/// Takes the pool rather than a connection: every publisher reads right
/// before its publish, after the connection its own write held has gone
/// back, so the read costs one acquire and holds nothing across the publish.
///
/// # Errors
/// Reports a pool that would not give a connection, or a statement that would
/// not run.
pub async fn fleet_counters(database: &Db, fleet_id: &str) -> Result<FleetCounters> {
    let mut connection = database.acquire().await?;
    fleet_counters_on(&mut connection, fleet_id).await
}

/// The fleet's counters as the database has them, read on `connection`.
///
/// Both columns are `COALESCE`d `BIGINT NOT NULL` read into `i64`, so the
/// decode cannot fail and a refused read is the statement's.
///
/// # Errors
/// Reports a statement that would not run.
pub async fn fleet_counters_on(
    connection: &mut PgConnection,
    fleet_id: &str,
) -> Result<FleetCounters> {
    let stored = sqlx::query_as::<_, StoredCounters>(SELECT_FLEET_COUNTERS)
        .bind(fleet_id)
        .fetch_one(&mut *connection)
        .await
        .map_err(query(CONTEXT_COUNTERS))?;
    Ok(FleetCounters {
        events_processed: stored.event_count,
        budget_used_nanos: stored.budget_used_nanos,
    })
}

/// The fleet's counters for a frame about to be published, or `None` with the
/// failure logged — never zeros.
pub async fn fleet_counters_best_effort(database: &Db, fleet_id: &str) -> Option<FleetCounters> {
    unread_logged(fleet_counters(database, fleet_id).await, fleet_id)
}

/// [`fleet_counters_best_effort`] on the connection a publisher's own write
/// held, so the hot path pays no second acquire.
pub async fn fleet_counters_best_effort_on(
    connection: &mut PgConnection,
    fleet_id: &str,
) -> Option<FleetCounters> {
    unread_logged(fleet_counters_on(connection, fleet_id).await, fleet_id)
}

/// The figures, or `None` with the refusal logged under its registry code.
fn unread_logged(read: Result<FleetCounters>, fleet_id: &str) -> Option<FleetCounters> {
    match read {
        Ok(counters) => Some(counters),
        Err(error) => {
            let Logged { error_code, reason } = error.logged();
            let event = EVENT_COUNTERS_UNREAD;
            tracing::warn!(error_code, fleet_id, reason, event);
            None
        }
    }
}
