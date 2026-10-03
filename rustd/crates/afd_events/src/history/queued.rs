//! A thread's waiting messages: admitted, on the fleet's queue, and not yet
//! handed to a runner.
//!
//! `core.fleet_events` gains a row only at lease, so a message sent to a busy
//! fleet has no history row until its turn. The admission ledger holds it the
//! whole time, and its undelivered rows are what every thread page adds, as
//! [`status::QUEUED`] rows, so a screen opened while a message waits agrees
//! with one that watched it arrive. A resumed page reads the ones older than
//! its cursor, so more waiting messages than one page holds still all appear.
//!
//! # One row per event, whichever read saw it
//!
//! The two reads are two statements, so a lease can land between them: the
//! same event then comes back from both. The history row wins, because it is
//! the later fact. The merged page is re-sorted newest-first on the history
//! read's own key, so the page cut and its cursor above stay honest.

use std::collections::HashSet;

use afd_admission::{Producer, logical_id};
use afd_core::event::status;
use afd_core::id::Uuid7;
use sqlx::PgConnection;
use sqlx::Row as _;
use sqlx::postgres::PgRow;

use super::cursor::Cursor;
use super::statement::{SELECT_THREAD_QUEUED, SELECT_THREAD_QUEUED_AFTER};
use super::{EventDetailRow, EventRow};
use crate::error::{self, Result, row_malformed};

const CONTEXT_QUEUED: &str = "read a fleet's waiting messages";

/// The fleet's waiting steers, newest first, at most `limit`, older than
/// `cursor` when a page resumes.
///
/// Steers only: a waiting webhook or schedule names no typed words, and its
/// row appears when a runner takes it, as it always has.
///
/// # Errors
/// Reports a datastore that would not answer, or a row this build cannot read.
pub(super) async fn waiting(
    connection: &mut PgConnection,
    workspace: &Uuid7,
    fleet: &Uuid7,
    cursor: Option<&Cursor>,
    limit: i64,
) -> Result<Vec<EventDetailRow>> {
    let scoped = match cursor {
        None => sqlx::query(SELECT_THREAD_QUEUED)
            .bind(workspace.as_str())
            .bind(fleet.as_str())
            .bind(Producer::Steer.as_str()),
        Some(at) => sqlx::query(SELECT_THREAD_QUEUED_AFTER)
            .bind(workspace.as_str())
            .bind(fleet.as_str())
            .bind(Producer::Steer.as_str())
            .bind(at.created_at)
            .bind(at.event_id.as_str()),
    };
    let rows = scoped
        .bind(limit)
        .fetch_all(connection)
        .await
        .map_err(error::query(CONTEXT_QUEUED))?;
    rows.iter().map(read).collect()
}

/// `delivered` and `waiting` as one page, newest first, each event once, cut
/// to the `bound` rows each read was asked for.
///
/// Each read returns up to `bound`, so the merge can hold twice that. The rows
/// past the cut are the oldest, and both reads find them again from the
/// cursor the page leaves, so cutting loses nothing.
pub(super) fn merged(
    delivered: Vec<EventDetailRow>,
    waiting: Vec<EventDetailRow>,
    bound: usize,
) -> Vec<EventDetailRow> {
    let seen: HashSet<&str> = delivered
        .iter()
        .map(|row| row.row.event_id.as_str())
        .collect();
    let fresh: Vec<EventDetailRow> = waiting
        .into_iter()
        .filter(|queued| !seen.contains(queued.row.event_id.as_str()))
        .collect();
    let mut page = delivered;
    if fresh.is_empty() {
        return page;
    }
    page.extend(fresh);
    page.sort_by(|a, b| {
        (b.row.created_at, &b.row.event_id).cmp(&(a.row.created_at, &a.row.event_id))
    });
    page.truncate(bound);
    page
}

/// One waiting admission as a thread row: its body, no answer yet.
fn read(row: &PgRow) -> Result<EventDetailRow> {
    let created_at: i64 = row.try_get(5).map_err(row_malformed("created_at"))?;
    let seq: i64 = row.try_get(6).map_err(row_malformed("seq"))?;
    Ok(EventDetailRow {
        row: EventRow {
            fleet_id: row.try_get(0).map_err(row_malformed("fleet_id"))?,
            event_id: logical_id(created_at, seq),
            workspace_id: row.try_get(1).map_err(row_malformed("workspace_id"))?,
            actor: row.try_get(2).map_err(row_malformed("actor"))?,
            event_type: row.try_get(3).map_err(row_malformed("event_type"))?,
            status: status::QUEUED.to_owned(),
            tokens: None,
            wall_ms: None,
            failure_label: None,
            failure_detail: None,
            checkpoint_id: None,
            resumes_event_id: None,
            created_at,
            updated_at: created_at,
            cost_nanos: None,
        },
        request_json: row.try_get(4).map_err(row_malformed("request_json"))?,
        response_text: None,
        tool_calls: None,
    })
}

#[cfg(test)]
#[path = "queued/tests.rs"]
mod tests;
