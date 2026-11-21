#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::atomic::{AtomicUsize, Ordering};

use af_core::id::Uuid7;
use af_core::test_util::trace::Capture;
use af_core::turn::status;
use af_wire::protocol::{Down, MESSAGE_BYTES_MAX};
use af_wire::run::{ANSWER_FAILED, ANSWER_FAILED_END, ANSWER_NONE, RunPayload, TaggedRun, Turn};
use af_wire::tool_detail::ItemStatus;
use af_wire::turn::TurnKind;
use afd_turns::view::Cursor;
use jiff::Timestamp;
use serde_json::json;

use super::{
    CallRow, EVENT_HISTORY_CUT, EVENT_HISTORY_UNAVAILABLE, FRAME_ENVELOPE_BYTES, FinishedTurn,
    HISTORY_PAGE_TURNS, Thread, fit_frame, turns_before,
};

/// The workspace, fleet and event every read here names.
const WORKSPACE: &str = "01924f4e-0000-7000-8000-000000000001";
const FLEET: &str = "01924f4e-0000-7000-8000-00000000fee7";
const EVENT: &str = "01924f4e-0000-7000-8000-0000000000e9";
/// The instant the read's own event came in: after every seeded row.
const EVENT_AT_MILLIS: u64 = 1_000_000;

/// A fleet chat's assignment, as the daemon frames one, with no history.
const PAYLOAD: &str = r#"{"run_id":"0190d5c2-7e1a-7000-8000-00000000e001","fencing_token":4,"run_expires_at":"1970-01-01T00:00:00.931Z","event":{"turn_id":"0190d5c2-7e1a-7000-8000-00000000e002","fleet_id":"01924f4e-0000-7000-8000-00000000fee7","workspace_id":"workspace_id","actor":"steer:user_1","event_type":"chat","request_json":"{\"message\":\"hi\"}","created_at":"1970-01-01T00:00:00.426Z"},"policy":{"network_policy":{"allow":[],"read_only":true,"read_post_paths":[]},"tools":[],"secrets_map":null,"mintable":[],"provider":"provider","api_key":"api_key","inference_host":"inference_host","base_url":null,"repository_binding":null,"http_origin_policies":[],"context":{"tool_window":0,"memory_checkpoint_every":0,"stage_chunk_threshold":0.75,"model":"model","context_cap_tokens":0}},"instructions":"Triage the queue.","history":[],"resume_hold":false,"inputs_waiting":false,"owner":{"kind":"fleet","fleet_id":"01924f4e-0000-7000-8000-00000000fee7"}}"#;

/// The instant `millis` after the epoch.
fn at(millis: u64) -> Timestamp {
    af_core::clock::after(
        Timestamp::UNIX_EPOCH,
        std::time::Duration::from_millis(millis),
    )
}

/// The turn seeded at `index`.
fn turn_id(index: u64) -> Uuid7 {
    Uuid7::parse(&format!("0199a0b0-0000-7000-8000-{index:012x}")).unwrap()
}

/// One thread row: `message` asked, answered `answer`, ended in `status`, at
/// sequence `index`.
fn row(index: u64, status: &str, message: &str, answer: Option<&str>) -> FinishedTurn {
    FinishedTurn {
        turn_id: turn_id(index),
        created_at: at(index),
        sequence: i64::try_from(index).unwrap(),
        status: status.to_owned(),
        failure_reason: None,
        request_json: json!({ "message": message }).to_string(),
        answer_text: answer.map(str::to_owned),
    }
}

/// `count` processed turns, numbered from 1, each asked `m{index}` and
/// answered `answer`.
fn processed(count: u64, answer: &str) -> Vec<FinishedTurn> {
    (1..=count)
        .map(|index| row(index, status::PROCESSED, &format!("m{index}"), Some(answer)))
        .collect()
}

/// Call `number` of the run at `fence`, made for turn `index`.
fn made(index: u64, fence: i64, number: i64, tool: &str, output: &str) -> CallRow {
    CallRow {
        turn_id: turn_id(index),
        fencing_token: fence,
        sequence: number,
        tool: tool.to_owned(),
        status: ItemStatus::Succeeded.as_str().to_owned(),
        arguments: json!({ "command": format!("step {number}") }).to_string(),
        output: output.to_owned(),
    }
}

/// A thread that pages `rows` as the statement does, newest first and
/// before the cursor, answers `calls` by turn, or fails; and counts its
/// page reads.
#[derive(Debug)]
struct Fake {
    rows: Option<Vec<FinishedTurn>>,
    calls: Vec<CallRow>,
    pages: AtomicUsize,
}

impl Fake {
    fn new(rows: Option<Vec<FinishedTurn>>) -> Self {
        Self {
            rows,
            calls: Vec::new(),
            pages: AtomicUsize::new(0),
        }
    }

    fn with_calls(mut self, calls: Vec<CallRow>) -> Self {
        self.calls = calls;
        self
    }
}

#[async_trait::async_trait]
impl Thread for Fake {
    async fn finished_before(
        &self,
        _workspace: &Uuid7,
        _fleet: &Uuid7,
        at: &Cursor,
        limit: i64,
    ) -> crate::Result<Vec<FinishedTurn>> {
        self.pages.fetch_add(1, Ordering::SeqCst);
        let rows = self.rows.as_ref().ok_or_else(crate::Error::run_not_found)?;
        let mut before: Vec<FinishedTurn> = rows
            .iter()
            .filter(|row| [status::PROCESSED, status::FLEET_ERROR].contains(&row.status.as_str()))
            .filter(|row| (row.created_at, &row.turn_id) < (at.created_at, &at.turn_id))
            .cloned()
            .collect();
        before.sort_by(|a, b| (b.created_at, &b.turn_id).cmp(&(a.created_at, &a.turn_id)));
        before.truncate(usize::try_from(limit).unwrap());
        Ok(before)
    }

    async fn calls_of(
        &self,
        _workspace: &Uuid7,
        _fleet: &Uuid7,
        turns: &[Uuid7],
    ) -> crate::Result<Vec<CallRow>> {
        Ok(self
            .calls
            .iter()
            .filter(|call| turns.contains(&call.turn_id))
            .cloned()
            .collect())
    }
}

/// Reads `thread` for a run whose event is `event_type`.
async fn turns_for(thread: &Fake, event_type: TurnKind) -> Vec<Turn<'static>> {
    let at = Cursor {
        created_at: at(EVENT_AT_MILLIS),
        turn_id: Uuid7::parse(EVENT).unwrap(),
    };
    let workspace = Uuid7::parse(WORKSPACE).unwrap();
    let fleet = Uuid7::parse(FLEET).unwrap();
    turns_before(thread, &workspace, &fleet, &at, event_type).await
}

/// The fixture assignment carrying `history`.
fn payload(history: Vec<Turn<'static>>) -> RunPayload<'static> {
    let mut payload: RunPayload<'static> = serde_json::from_str(PAYLOAD).unwrap();
    payload.history = history;
    payload
}

/// Only finished rows become turns, oldest first; a failure answers with
/// its label, and a run that left no reply answers `[no reply]`.
#[tokio::test]
async fn test_history_keeps_finished_turns_only() {
    let mut failed = row(3, status::FLEET_ERROR, "third", None);
    failed.failure_reason = Some("timeout_kill".to_owned());
    let thread = Fake::new(Some(vec![
        row(1, status::PROCESSED, "first", Some("one")),
        row(2, status::PROCESSED, "second", None),
        failed,
        row(4, "queued", "waiting", None),
    ]));

    let turns = turns_for(&thread, TurnKind::Chat).await;

    let read: Vec<(u64, &str, &str)> = turns
        .iter()
        .map(|turn| (turn.sequence, turn.message.as_ref(), turn.answer.as_ref()))
        .collect();
    assert_eq!(
        read,
        [
            (1, "first", "one"),
            (2, "second", ANSWER_NONE),
            (3, "third", "[the run failed: timeout_kill]"),
        ]
    );
}

/// Twenty finished turns of 20 KiB each, past every count and byte cap the
/// history once had, are all carried, oldest first, and none is cut.
#[tokio::test]
async fn a_long_chat_run_carries_every_finished_turn() {
    const TURNS: u64 = 20;
    const TURN_BYTES: usize = 20 * 1024;
    let answer = "a".repeat(TURN_BYTES);
    let thread = Fake::new(Some(processed(TURNS, &answer)));

    let turns = turns_for(&thread, TurnKind::Chat).await;

    assert_eq!(turns.len(), 20, "every finished turn is carried");
    let sequences: Vec<u64> = turns.iter().map(|turn| turn.sequence).collect();
    assert_eq!(sequences, (1..=TURNS).collect::<Vec<_>>(), "oldest first");
    for turn in &turns {
        assert_eq!(
            turn.answer.len(),
            TURN_BYTES,
            "turn {} was cut",
            turn.sequence
        );
    }
    let fitted = fit_frame(payload(turns));
    assert_eq!(fitted.history.len(), 20, "the frame cut none");
}

/// A finished turn whose run made two calls carries both, each with its
/// arguments and output, in the order the run made them, under the id the
/// thread names it by.
#[tokio::test]
async fn a_follow_up_run_carries_the_earlier_turns_tool_calls() {
    let thread = Fake::new(Some(vec![
        row(
            1,
            status::PROCESSED,
            "which tests failed?",
            Some("two: a and b"),
        ),
        row(2, status::PROCESSED, "thanks", Some("any time")),
    ]))
    .with_calls(vec![
        made(1, 7, 1, "shell", "FAIL a\nFAIL b"),
        made(1, 7, 2, "read_file", "fn a() {}"),
    ]);

    let turns = turns_for(&thread, TurnKind::Chat).await;

    let calls: Vec<(&str, &str, &str, ItemStatus)> = turns[0]
        .calls
        .iter()
        .map(|call| {
            (
                call.call_id.as_ref(),
                call.tool.as_ref(),
                call.output.as_ref(),
                call.status,
            )
        })
        .collect();
    assert_eq!(
        calls,
        [
            ("7:1", "shell", "FAIL a\nFAIL b", ItemStatus::Succeeded),
            ("7:2", "read_file", "fn a() {}", ItemStatus::Succeeded),
        ]
    );
    assert_eq!(
        turns[0].calls[1].arguments.get("command"),
        Some(&json!("step 2"))
    );
    assert!(turns[1].calls.is_empty(), "the second turn called nothing");
}

/// A thread longer than one page is read page after page to its first turn.
#[tokio::test]
async fn test_a_thread_past_one_page_is_read_whole() {
    let count = u64::try_from(HISTORY_PAGE_TURNS).unwrap() * 2 + 50;
    let thread = Fake::new(Some(processed(count, "a")));

    let turns = turns_for(&thread, TurnKind::Chat).await;

    assert_eq!(u64::try_from(turns.len()).unwrap(), count);
    assert_eq!(turns[0].message, "m1");
    assert_eq!(
        thread.pages.load(Ordering::SeqCst),
        3,
        "two full pages and a short one"
    );
}

/// A read stops paging once what it read passes one frame: older turns
/// could never ride the assignment.
#[tokio::test]
async fn test_a_read_stops_past_one_frame() {
    let page = u64::try_from(HISTORY_PAGE_TURNS).unwrap();
    // The newest page alone passes the frame; the page before it is small.
    let heavy = "b".repeat(MESSAGE_BYTES_MAX / usize::try_from(page).unwrap() + 1);
    let mut rows = processed(page, "a");
    rows.extend(
        (page + 1..=page * 2)
            .map(|index| row(index, status::PROCESSED, &format!("m{index}"), Some(&heavy))),
    );
    let thread = Fake::new(Some(rows));

    let turns = turns_for(&thread, TurnKind::Chat).await;

    assert_eq!(
        thread.pages.load(Ordering::SeqCst),
        1,
        "one page passed the frame"
    );
    assert_eq!(
        turns.first().map(|turn| turn.sequence),
        Some(page + 1),
        "the newest turns are the ones read"
    );
}

/// A failed run whose row names no failure answers with the failed status
/// itself, so the model still reads that the turn failed.
#[tokio::test]
async fn test_a_failure_without_a_label_answers_with_its_status() {
    let thread = Fake::new(Some(vec![row(1, status::FLEET_ERROR, "asked", None)]));

    let turns = turns_for(&thread, TurnKind::Chat).await;

    assert_eq!(
        turns[0].answer,
        format!("{ANSWER_FAILED}{}{ANSWER_FAILED_END}", status::FLEET_ERROR)
    );
}

/// Webhook, cron and continuation runs carry no turns, and never read.
#[tokio::test]
async fn test_non_chat_run_carries_no_history() {
    for event_type in [TurnKind::Webhook, TurnKind::Cron, TurnKind::Continuation] {
        let thread = Fake::new(Some(vec![row(1, status::PROCESSED, "m", Some("a"))]));
        let found = turns_for(&thread, event_type).await;
        assert!(found.is_empty(), "{found:?}");
        assert_eq!(
            thread.pages.load(Ordering::SeqCst),
            0,
            "{event_type:?} never reads the thread"
        );
    }
}

/// Held by every test that reads a process-wide history counter: one test's
/// count would land inside the window another test measures it in.
static COUNTED: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A read that fails issues the run with no turns, and says so once.
#[tokio::test]
async fn test_history_read_failure_fails_open() {
    let _counted_alone = COUNTED.lock().await;
    let capture = Capture::install();
    let thread = Fake::new(None);

    let turns = turns_for(&thread, TurnKind::Chat).await;

    assert!(turns.is_empty(), "{turns:?}");
    let logged = capture.only(EVENT_HISTORY_UNAVAILABLE);
    assert_eq!(logged.field("fleet_id"), Some(FLEET));
    assert!(logged.field("error_code").is_some(), "{logged:?}");
}

/// The frame envelope around a payload, the message and run tags, is within
/// the room [`fit_frame`] keeps for it.
#[test]
fn test_the_frame_envelope_is_within_its_room() {
    let bare = af_validate::serialized_len(&payload(Vec::new())).unwrap();
    let frame = Down::Run {
        run: TaggedRun::Fleet(Box::new(payload(Vec::new()))),
    };
    let framed = af_validate::serialized_len(&frame).unwrap();

    assert!(framed > bare, "the frame wraps the payload");
    assert!(
        framed - bare <= FRAME_ENVELOPE_BYTES,
        "{} bytes of envelope",
        framed - bare
    );
}

/// A history past the run frame drops its oldest turns until the framed
/// assignment fits, keeps the newest, and the cut is logged and counted
/// once, under `frame`.
#[tokio::test]
async fn test_a_history_past_the_frame_drops_its_oldest_turns() {
    use af_observability::test_util::Capture as Metrics;

    const CUTS: &str = "agentsfleet_run_history_cuts_total";
    const TURNS: u64 = 40;
    let _counted_alone = COUNTED.lock().await;
    let metrics = Metrics::install();
    let capture = Capture::install();
    let before = metrics.sum(CUTS, &[("reason", "frame")]);
    // Forty 1 MiB answers pass the 32 MiB frame.
    let answer = "c".repeat(1 << 20);
    let history = (1..=TURNS)
        .map(|sequence| Turn {
            sequence,
            message: format!("m{sequence}").into(),
            calls: Vec::new(),
            answer: answer.clone().into(),
        })
        .collect();

    let fitted = fit_frame(payload(history));

    let framed = af_validate::serialized_len(&fitted).unwrap() + FRAME_ENVELOPE_BYTES;
    assert!(framed <= MESSAGE_BYTES_MAX, "{framed} bytes framed");
    let kept = u64::try_from(fitted.history.len()).unwrap();
    assert!((1..TURNS).contains(&kept), "{kept} turns kept");
    assert_eq!(
        fitted.history.last().map(|turn| turn.sequence),
        Some(TURNS),
        "the newest turn stays"
    );
    assert_eq!(
        fitted.history.first().map(|turn| turn.sequence),
        Some(TURNS + 1 - kept),
        "only the oldest go"
    );
    let logged = capture.only(EVENT_HISTORY_CUT);
    assert_eq!(logged.field("fleet_id"), Some(FLEET));
    assert_eq!(metrics.sum(CUTS, &[("reason", "frame")]), before + 1);
}

/// Every chat run's read bytes are observed, and a failed read counts as
/// one run issued without its turns.
#[tokio::test]
async fn test_history_metrics_recorded() {
    use af_observability::test_util::Capture as Metrics;

    const BYTES: &str = "agentsfleet_run_history_bytes";
    const FAILURES: &str = "agentsfleet_run_history_read_failures_total";
    let _counted_alone = COUNTED.lock().await;
    let metrics = Metrics::install();
    let before = (
        metrics.histogram_count(BYTES, &[]),
        metrics.sum(FAILURES, &[]),
        metrics.histogram_sum(BYTES, &[]),
    );

    let rows = processed(3, "answer");
    let stored: usize = rows
        .iter()
        .map(|row| row.request_json.len() + row.answer_text.as_ref().map_or(0, String::len))
        .sum();
    turns_for(&Fake::new(Some(rows)), TurnKind::Chat).await;
    turns_for(&Fake::new(None), TurnKind::Chat).await;

    assert_eq!(
        metrics.histogram_count(BYTES, &[]),
        before.0 + 1,
        "one observed run"
    );
    assert_eq!(metrics.sum(FAILURES, &[]), before.1 + 1);
    let observed = metrics.histogram_sum(BYTES, &[]) - before.2;
    let expected = f64::from(u32::try_from(stored).unwrap());
    assert!(
        (observed - expected).abs() < f64::EPSILON,
        "observed {observed} bytes; the read stored {expected}"
    );
}
