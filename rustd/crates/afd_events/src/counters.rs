//! A fleet's activity counters, read right before a frame carries them.
//!
//! Every frame the daemon publishes on a fleet's tail carries an absolute
//! counter snapshot (`afd_wire::tail::FleetCounters`), and the publishers
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

use afd_wire::tail::FleetCounters;
use sqlx::PgConnection;
use sqlx::Row as _;

use crate::error::{Result, query, row_malformed};
use crate::sql::SELECT_FLEET_COUNTERS;

/// The context a refused counter read is reported under.
const CONTEXT_COUNTERS: &str = "fleet activity counters";

/// The log event a publisher emits when it goes out without its figures.
const EVENT_COUNTERS_UNREAD: &str = "fleet_counters_unread";

/// The fleet's counters as the database has them.
///
/// # Errors
/// Reports a statement that would not run, or a row this build cannot read.
pub async fn fleet_counters(
    connection: &mut PgConnection,
    fleet_id: &str,
) -> Result<FleetCounters> {
    let row = sqlx::query(SELECT_FLEET_COUNTERS)
        .bind(fleet_id)
        .fetch_one(&mut *connection)
        .await
        .map_err(query(CONTEXT_COUNTERS))?;
    Ok(FleetCounters {
        events_processed: row.try_get(0).map_err(row_malformed("events_processed"))?,
        budget_used_nanos: row.try_get(1).map_err(row_malformed("budget_used_nanos"))?,
    })
}

/// The fleet's counters for a frame about to be published, or `None` with the
/// failure logged — never zeros.
pub async fn fleet_counters_best_effort(
    connection: &mut PgConnection,
    fleet_id: &str,
) -> Option<FleetCounters> {
    match fleet_counters(connection, fleet_id).await {
        Ok(counters) => Some(counters),
        Err(error) => {
            let code = error.code().as_str();
            let reason = error.to_string();
            let event = EVENT_COUNTERS_UNREAD;
            tracing::warn!(error_code = code, fleet_id, reason, event);
            None
        }
    }
}
