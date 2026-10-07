#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_core::event::status;
use afd_core::id::Uuid7;
use afd_core::test_util::trace::Capture;
use afd_events::{Cursor, EventDetailRow};
use afd_wire::event::EventType;
use afd_wire::lease::{
    ANSWER_NONE, HISTORY_BYTES_MAX, HISTORY_TURNS_MAX, TURN_TEXT_BYTES_MAX, Turn,
};

use super::{EVENT_HISTORY_UNAVAILABLE, Thread, turns_before, within_caps};

/// The workspace, fleet and event every read here names.
const WORKSPACE: &str = "01924f4e-0000-7000-8000-000000000001";
const FLEET: &str = "01924f4e-0000-7000-8000-00000000fee7";
const EVENT: &str = "01924f4e-0000-7000-8000-0000000000e9";

/// One thread row: `message` asked, answered `answer`, ended in `status`.
fn row(index: i64, status: &str, message: &str, answer: Option<&str>) -> EventDetailRow {
    let mut row = EventDetailRow::fixture(&format!("e{index}"), index, String::new());
    status.clone_into(&mut row.row.status);
    row.request_json = serde_json::json!({ "message": message }).to_string();
    row.response_text = answer.map(str::to_owned);
    row
}

/// A thread that answers `rows`, or fails, and counts how often it was read.
#[derive(Debug)]
struct Fake {
    rows: Option<Vec<EventDetailRow>>,
    reads: std::sync::atomic::AtomicUsize,
}

impl Fake {
    fn new(rows: Option<Vec<EventDetailRow>>) -> Self {
        Self {
            rows,
            reads: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl Thread for Fake {
    async fn before(
        &self,
        _workspace: &Uuid7,
        _fleet: &Uuid7,
        _at: &Cursor,
        _limit: i64,
    ) -> afd_events::Result<Vec<EventDetailRow>> {
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        match &self.rows {
            Some(rows) => Ok(rows.clone()),
            None => Err(afd_events::error::one_of_each_kind().remove(0).1),
        }
    }
}

/// Reads `thread` for a lease whose event is `event_type`.
async fn turns_for(thread: &Fake, event_type: EventType) -> Vec<Turn<'static>> {
    let at = Cursor {
        created_at: 100,
        event_id: EVENT.to_owned(),
    };
    let workspace = Uuid7::parse(WORKSPACE).unwrap();
    let fleet = Uuid7::parse(FLEET).unwrap();
    turns_before(thread, &workspace, &fleet, &at, event_type).await
}

/// Only finished rows become turns, oldest first; a failure answers with
/// its label, and a run that left no reply answers `[no reply]`.
#[test]
fn test_history_keeps_finished_turns_only() {
    let mut failed = row(3, status::FLEET_ERROR, "third", None);
    failed.row.failure_label = Some("timeout_kill".to_owned());
    let newest_first = vec![
        row(4, "queued", "waiting", None),
        failed,
        row(2, status::PROCESSED, "second", None),
        row(1, status::PROCESSED, "first", Some("one")),
    ];

    let turns = within_caps(newest_first).turns;

    let read: Vec<(&str, &str)> = turns
        .iter()
        .map(|turn| (turn.message.as_ref(), turn.answer.as_ref()))
        .collect();
    assert_eq!(
        read,
        [
            ("first", "one"),
            ("second", ANSWER_NONE),
            ("third", "[the run failed: timeout_kill]"),
        ]
    );
}

/// At most eight turns, the newest kept; each text cut on a character
/// boundary; and the oldest dropped until the rest fit the byte budget.
#[test]
fn test_history_caps_turns_and_bytes() {
    let rows: Vec<EventDetailRow> = (1..=9)
        .rev()
        .map(|index| row(index, status::PROCESSED, &format!("m{index}"), Some("a")))
        .collect();
    let kept = within_caps(rows).turns;
    assert_eq!(kept.len(), HISTORY_TURNS_MAX);
    assert_eq!(kept[0].message, "m2", "the oldest of nine goes first");

    // A message one two-byte character past the cap is cut before it.
    let long = format!("{}é", "x".repeat(TURN_TEXT_BYTES_MAX - 1));
    let cut = within_caps(vec![row(1, status::PROCESSED, &long, Some("a"))]).turns;
    assert_eq!(cut[0].message.len(), TURN_TEXT_BYTES_MAX - 1);

    // Eight turns at the text cap each pass the budget; the oldest go first.
    let full = "y".repeat(TURN_TEXT_BYTES_MAX);
    let heavy: Vec<EventDetailRow> = (1..=8)
        .rev()
        .map(|index| row(index, status::PROCESSED, &format!("m{index}"), Some(&full)))
        .collect();
    let fitted = within_caps(heavy).turns;
    let bytes: usize = fitted
        .iter()
        .map(|turn| turn.message.len() + turn.answer.len())
        .sum();
    assert!(bytes <= HISTORY_BYTES_MAX, "{bytes} bytes kept");
    assert_eq!(
        fitted.last().map(|turn| turn.message.as_ref()),
        Some("m8"),
        "the newest turn survives the budget"
    );
    assert!(fitted.len() < 8, "some oldest turns were dropped");
}

/// Webhook, cron and continuation leases carry no turns, and never read.
#[tokio::test]
async fn test_non_chat_lease_carries_no_history() {
    for event_type in [EventType::Webhook, EventType::Cron, EventType::Continuation] {
        let thread = Fake::new(Some(vec![row(1, status::PROCESSED, "m", Some("a"))]));
        let found = turns_for(&thread, event_type).await;
        assert!(found.is_empty(), "{found:?}");
        assert_eq!(
            thread.reads.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "{event_type:?} never reads the thread"
        );
    }
}

/// A read that fails issues the lease with no turns, and says so once.
#[tokio::test]
async fn test_history_read_failure_fails_open() {
    let capture = Capture::install();
    let thread = Fake::new(None);

    let turns = turns_for(&thread, EventType::Chat).await;

    assert!(turns.is_empty(), "{turns:?}");
    let logged = capture.only(EVENT_HISTORY_UNAVAILABLE);
    assert_eq!(logged.field("fleet_id"), Some(FLEET));
    assert!(logged.field("error_code").is_some(), "{logged:?}");
}

/// Every chat lease's bytes are observed, each cap that cut counts once, and
/// a failed read counts as one lease issued without its turns.
#[tokio::test]
async fn test_history_metrics_recorded() {
    use afd_observability::test_util::Capture as Metrics;

    const BYTES: &str = "agentsfleet_lease_history_bytes";
    const CUTS: &str = "agentsfleet_lease_history_cuts_total";
    const FAILURES: &str = "agentsfleet_lease_history_read_failures_total";
    let metrics = Metrics::install();
    let before = (
        metrics.histogram_count(BYTES, &[]),
        metrics.sum(CUTS, &[("reason", "turns")]),
        metrics.sum(CUTS, &[("reason", "text")]),
        metrics.sum(CUTS, &[("reason", "bytes")]),
        metrics.sum(FAILURES, &[]),
    );

    // Nine finished turns, one of them past the text cap: two caps cut.
    let long = "z".repeat(TURN_TEXT_BYTES_MAX + 1);
    let rows: Vec<EventDetailRow> = (1..=9)
        .rev()
        .map(|index| row(index, status::PROCESSED, &long, Some("a")))
        .collect();
    turns_for(&Fake::new(Some(rows)), EventType::Chat).await;
    turns_for(&Fake::new(None), EventType::Chat).await;

    assert_eq!(
        metrics.histogram_count(BYTES, &[]),
        before.0 + 1,
        "one observed lease"
    );
    assert_eq!(metrics.sum(CUTS, &[("reason", "turns")]), before.1 + 1);
    assert_eq!(
        metrics.sum(CUTS, &[("reason", "text")]),
        before.2 + 1,
        "counted once, not per text"
    );
    assert_eq!(
        metrics.sum(CUTS, &[("reason", "bytes")]),
        before.3 + 1,
        "eight 16 KiB texts pass the budget"
    );
    assert_eq!(metrics.sum(FAILURES, &[]), before.4 + 1);
}
