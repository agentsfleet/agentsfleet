//! A thread's waiting messages: admitted, on the fleet's queue, and not yet
//! handed to a runner.
//!
//! `fleet.events` gains a row only at lease, so a message sent to a busy
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
//!
//! # A waiting row already wears its history key
//!
//! An admitted event's id is its admission row's id, and the lease stamps the
//! history row with the admission's `created_at`. So the waiting row reads both
//! straight off `fleet.admissions` and needs no translation: the event the
//! thread lists while it waits is the event, by id and instant, that it lists
//! once a runner has it.

use std::collections::HashSet;

use afd_admission::Producer;
use afd_core::event::status;
use afd_core::id::Uuid7;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::PgConnection;

use super::cursor::Cursor;
use super::statement::{SELECT_THREAD_QUEUED, SELECT_THREAD_QUEUED_AFTER};
use super::{EventDetailRow, EventRow};
use crate::error::{self, Result};

const CONTEXT_QUEUED: &str = "read a fleet's waiting messages";

/// One waiting admission, as `fleet.admissions` spells it.
#[derive(sqlx::FromRow)]
struct Waiting {
    event_id: String,
    fleet_id: String,
    workspace_id: String,
    actor: String,
    event_type: String,
    request_json: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
}

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
        None => sqlx::query_as::<_, Waiting>(SELECT_THREAD_QUEUED)
            .bind(workspace.as_str())
            .bind(fleet.as_str())
            .bind(Producer::Steer.as_str()),
        Some(at) => sqlx::query_as::<_, Waiting>(SELECT_THREAD_QUEUED_AFTER)
            .bind(workspace.as_str())
            .bind(fleet.as_str())
            .bind(Producer::Steer.as_str())
            .bind(at.created_at.to_sqlx())
            .bind(at.event_id.as_str()),
    };
    let rows = scoped
        .bind(limit)
        .fetch_all(connection)
        .await
        .map_err(error::query(CONTEXT_QUEUED))?;
    Ok(rows.into_iter().map(Waiting::into_thread_row).collect())
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

impl Waiting {
    /// This admission as a thread row: its body, no answer yet.
    fn into_thread_row(self) -> EventDetailRow {
        EventDetailRow {
            row: EventRow {
                fleet_id: self.fleet_id,
                event_id: self.event_id,
                workspace_id: self.workspace_id,
                actor: self.actor,
                event_type: self.event_type,
                status: status::QUEUED.to_owned(),
                tokens: None,
                wall_ms: None,
                failure_label: None,
                failure_detail: None,
                resumes_event_id: None,
                created_at: self.created_at,
                updated_at: self.created_at,
                cost_nanos: None,
            },
            request_json: self.request_json,
            response_text: None,
            tool_calls: None,
        }
    }
}

#[cfg(test)]
mod tests;
