//! How a first page folds the waiting messages into the delivered ones.

use afd_core::event::status;

use super::merged;
use crate::history::{EventDetailRow, EventRow};

/// A bound wider than any page these cases build, so only the cut case cuts.
const WIDE: usize = 10;

/// A thread row for `event_id` at `created_at`, in `state`.
fn row(event_id: &str, created_at: i64, state: &str) -> EventDetailRow {
    EventDetailRow {
        row: EventRow {
            fleet_id: String::new(),
            event_id: event_id.to_owned(),
            workspace_id: String::new(),
            actor: String::new(),
            event_type: String::new(),
            status: state.to_owned(),
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
        request_json: String::new(),
        response_text: None,
        tool_calls: None,
    }
}

fn ids(page: &[EventDetailRow]) -> Vec<(&str, &str)> {
    page.iter()
        .map(|row| (row.row.event_id.as_str(), row.row.status.as_str()))
        .collect()
}

#[test]
fn should_lead_the_page_with_a_message_still_waiting() {
    let page = merged(
        vec![row("10-1", 10, status::PROCESSED)],
        vec![row("20-2", 20, status::QUEUED)],
        WIDE,
    );
    assert_eq!(
        ids(&page),
        [("20-2", status::QUEUED), ("10-1", status::PROCESSED)]
    );
}

/// A lease between the two reads returns one event from both: the history
/// row is the later fact and the only one kept.
#[test]
fn should_keep_the_history_row_when_both_reads_saw_one_event() {
    let page = merged(
        vec![row("20-2", 20, status::RECEIVED)],
        vec![row("20-2", 20, status::QUEUED)],
        WIDE,
    );
    assert_eq!(ids(&page), [("20-2", status::RECEIVED)]);
}

/// A waiting message older than a delivered one sits in created order, so the
/// cursor the page cut writes is a true boundary.
#[test]
fn should_order_the_merged_page_on_the_history_key() {
    let page = merged(
        vec![
            row("30-3", 30, status::RECEIVED),
            row("10-1", 10, status::PROCESSED),
        ],
        vec![row("20-2", 20, status::QUEUED)],
        WIDE,
    );
    assert_eq!(
        ids(&page),
        [
            ("30-3", status::RECEIVED),
            ("20-2", status::QUEUED),
            ("10-1", status::PROCESSED)
        ]
    );
}

/// Each read returns up to the bound, so the merge can hold twice it; the page
/// keeps the newest `bound` and leaves the oldest for the cursor to find again.
#[test]
fn should_cut_the_merged_page_to_the_bound_keeping_the_newest() {
    let page = merged(
        vec![
            row("40-4", 40, status::RECEIVED),
            row("10-1", 10, status::PROCESSED),
        ],
        vec![
            row("30-3", 30, status::QUEUED),
            row("20-2", 20, status::QUEUED),
        ],
        2,
    );
    assert_eq!(
        ids(&page),
        [("40-4", status::RECEIVED), ("30-3", status::QUEUED)]
    );
}

/// Every waiting row already delivered is dropped, not only the first match,
/// and a delivered page with nothing fresh comes back as it was.
#[test]
fn should_drop_every_waiting_row_the_history_read_already_returned() {
    let page = merged(
        vec![
            row("20-2", 20, status::RECEIVED),
            row("10-1", 10, status::PROCESSED),
        ],
        vec![
            row("20-2", 20, status::QUEUED),
            row("10-1", 10, status::QUEUED),
        ],
        WIDE,
    );
    assert_eq!(
        ids(&page),
        [("20-2", status::RECEIVED), ("10-1", status::PROCESSED)]
    );
}
