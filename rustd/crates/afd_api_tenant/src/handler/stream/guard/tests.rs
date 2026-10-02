//! The fleet stream's re-check, against a question that answers from a
//! script: each arm of the answer, a run of them, the ceiling on unanswered
//! checks, and what each logged, on a paused clock.
#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use afd_core::error_code::{self, ErrorCode};
use afd_core::id::Uuid7;
use afd_core::test_util::trace::{Capture, CapturedEvent};
use afd_http::handler::Refusable;
use afd_sse::{Frame, KIND_ACCESS_REVOKED};
use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};

use super::super::revocable::{
    EVENT_RECHECK_DEFERRED, EVENT_STREAM_UNVERIFIED, MAX_DEFERRED_RECHECKS, REASON_BUDGET,
    RECHECK_BUDGET, STREAM_FLEET,
};
use super::{RECHECK_INTERVAL, Recheck, watched};

const WORKSPACE: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1011";

/// What the fixture outage says about itself.
const OUTAGE_REASON: &str = "the pool timed out";

/// The fixture outage's code: not the budget's, so a record shows which arm
/// logged it.
const OUTAGE_CODE: ErrorCode = error_code::INTERNAL_DB_QUERY;

/// The code a re-check past its budget is logged with.
const BUDGET_CODE: ErrorCode = error_code::INTERNAL_DB_UNAVAILABLE;

/// Past one re-check and its budget, short of the next re-check.
const ONE_BEAT: Duration = RECHECK_INTERVAL.saturating_add(RECHECK_BUDGET.saturating_mul(2));

/// A datastore that would not answer.
#[derive(Debug)]
struct Outage;

impl Refusable for Outage {
    fn code(&self) -> ErrorCode {
        OUTAGE_CODE
    }
    fn detail(&self) -> &'static str {
        "unavailable"
    }
    fn is_datastore_unavailable(&self) -> bool {
        true
    }
    fn reason(&self) -> String {
        OUTAGE_REASON.to_owned()
    }
}

/// How the fixture question answers one beat.
#[derive(Debug, Clone, Copy)]
enum Answer {
    Admitted,
    Revoked,
    Unreachable,
    Silent,
}

/// A question answering from a script, one answer per beat; past its end it
/// repeats the last. `asked` indexes the script, because `admitted` takes
/// `&self`.
struct Scripted {
    answers: Vec<Answer>,
    asked: AtomicUsize,
    workspace: Uuid7,
}

impl Recheck for Scripted {
    type Error = Outage;

    async fn admitted(&self) -> Result<bool, Outage> {
        let turn = self.asked.fetch_add(1, Ordering::Relaxed);
        let answer = self.answers.get(turn).or(self.answers.last());
        match answer.expect("every script has an answer") {
            Answer::Admitted => Ok(true),
            Answer::Revoked => Ok(false),
            Answer::Unreachable => Err(Outage),
            Answer::Silent => std::future::pending().await,
        }
    }

    fn workspace(&self) -> &Uuid7 {
        &self.workspace
    }
}

/// A question answering `answers` in turn.
fn scripted(answers: &[Answer]) -> Scripted {
    Scripted {
        answers: answers.to_vec(),
        asked: AtomicUsize::new(0),
        workspace: Uuid7::parse(WORKSPACE).expect("the fixture identifier is UUIDv7"),
    }
}

/// A stream with no frames of its own, guarded by a question answering
/// `answers` in turn.
fn quiet(answers: &[Answer]) -> BoxStream<'static, Frame> {
    watched(stream::pending().boxed(), scripted(answers))
}

/// Whether the stream is still open after `beats` re-checks from now:
/// nothing sent, not ended.
async fn open_through(stream: &mut BoxStream<'static, Frame>, beats: u32) -> bool {
    let span = RECHECK_INTERVAL
        .saturating_mul(beats)
        .saturating_add(RECHECK_BUDGET.saturating_mul(2));
    tokio::time::timeout(span, stream.next()).await.is_err()
}

/// Whether the stream ends within one more beat without sending anything,
/// `access_revoked` included.
async fn ends_quietly_within_a_beat(stream: &mut BoxStream<'static, Frame>) -> bool {
    matches!(
        tokio::time::timeout(ONE_BEAT, stream.next()).await,
        Ok(None)
    )
}

/// The stream's next frame, within one beat, is `access_revoked`, and nothing
/// follows it.
async fn assert_ends_on_access_revoked(stream: &mut BoxStream<'static, Frame>) {
    let last = tokio::time::timeout(ONE_BEAT, stream.next())
        .await
        .expect("the beat answers")
        .expect("the revocation is sent");
    assert_eq!(last.kind, KIND_ACCESS_REVOKED);
    assert!(stream.next().await.is_none(), "nothing follows it");
}

/// The unanswered checks a stream survives: one fewer than the ceiling.
fn survivable() -> usize {
    usize::try_from(MAX_DEFERRED_RECHECKS - 1).expect("the ceiling is a small count")
}

/// Every record raised under `event`, in order.
fn records(capture: &Capture, event: &str) -> Vec<CapturedEvent> {
    let mut all = capture.events();
    all.retain(|record| record.field("event") == Some(event));
    all
}

/// Exactly `count` deferrals were logged.
fn assert_deferrals(capture: &Capture, count: usize) {
    let deferred = records(capture, EVENT_RECHECK_DEFERRED);
    assert_eq!(deferred.len(), count, "{deferred:?}");
}

/// A record of a re-check nobody answered: `warn`, the code and reason of
/// the arm that wrote it, the workspace, and nothing that names a person.
fn assert_unanswered(record: &CapturedEvent, code: ErrorCode, reason: &str) {
    assert_eq!(record.level, tracing::Level::WARN);
    assert_eq!(record.field("error_code"), Some(code.as_str()));
    assert_eq!(record.field("workspace_id"), Some(WORKSPACE));
    assert_eq!(
        record.field("stream"),
        Some(STREAM_FLEET),
        "the record names its stream"
    );
    assert_eq!(record.field("reason"), Some(reason));
    assert!(
        record.fields.values().all(|value| !value.contains('@')),
        "no address in the record: {record:?}"
    );
}

/// A datastore that will not answer keeps the stream, and the deferral is
/// logged once with the outage's own code.
#[tokio::test(start_paused = true)]
async fn should_keep_the_stream_and_log_once_when_the_recheck_fails() {
    let capture = Capture::install();
    let mut stream = quiet(&[Answer::Unreachable]);
    assert!(open_through(&mut stream, 1).await);
    let deferred = capture.only(EVENT_RECHECK_DEFERRED);
    assert_unanswered(&deferred, OUTAGE_CODE, OUTAGE_REASON);
}

/// A re-check past its budget keeps the stream, and the deferral is logged
/// once as the datastore not answering in time.
#[tokio::test(start_paused = true)]
async fn should_keep_the_stream_and_log_once_when_the_recheck_overruns_its_budget() {
    let capture = Capture::install();
    let mut stream = quiet(&[Answer::Silent]);
    assert!(open_through(&mut stream, 1).await);
    let deferred = capture.only(EVENT_RECHECK_DEFERRED);
    assert_unanswered(&deferred, BUDGET_CODE, REASON_BUDGET);
}

/// A caller still admitted keeps the stream past the ceiling on unanswered
/// checks, and nothing is deferred or closed.
#[tokio::test(start_paused = true)]
async fn should_keep_the_stream_quietly_while_the_caller_is_admitted() {
    let capture = Capture::install();
    let mut stream = quiet(&[Answer::Admitted]);
    assert!(open_through(&mut stream, MAX_DEFERRED_RECHECKS + 1).await);
    assert_deferrals(&capture, 0);
    assert!(records(&capture, EVENT_STREAM_UNVERIFIED).is_empty());
}

/// A caller who lost the workspace gets `access_revoked`, then the end.
#[tokio::test(start_paused = true)]
async fn should_end_on_access_revoked_once_the_caller_is_refused() {
    let mut stream = quiet(&[Answer::Revoked]);
    assert_ends_on_access_revoked(&mut stream).await;
}

/// A revocation behind a re-check that overran its budget lands on the next
/// beat: the overrun delays it by one beat, never swallows it.
#[tokio::test(start_paused = true)]
async fn should_end_on_access_revoked_on_the_beat_after_an_overrun() {
    let capture = Capture::install();
    let mut stream = quiet(&[Answer::Silent, Answer::Revoked]);
    assert!(open_through(&mut stream, 1).await, "the overrun keeps it");
    assert_ends_on_access_revoked(&mut stream).await;
    let deferred = capture.only(EVENT_RECHECK_DEFERRED);
    assert_unanswered(&deferred, BUDGET_CODE, REASON_BUDGET);
}

/// An answered re-check starts the count again: the ceiling counts
/// unanswered checks in a row, not unanswered checks ever.
#[tokio::test(start_paused = true)]
async fn should_count_from_zero_again_after_an_admitted_answer() {
    let capture = Capture::install();
    let mut script = vec![Answer::Unreachable, Answer::Admitted];
    script.extend([Answer::Unreachable].repeat(survivable()));
    let mut stream = quiet(&script);

    assert!(open_through(&mut stream, MAX_DEFERRED_RECHECKS + 1).await);
    assert!(ends_quietly_within_a_beat(&mut stream).await);
    assert_deferrals(&capture, survivable() + 1);
    let closing = capture.only(EVENT_STREAM_UNVERIFIED);
    assert_unanswered(&closing, OUTAGE_CODE, OUTAGE_REASON);
}

/// The ceiling on unanswered checks ends the stream with no frame — an
/// outage is not a revocation — and the closing is logged once, with the
/// code and reason of the check that hit the ceiling.
async fn should_end_quietly_at_the_ceiling(answer: Answer, code: ErrorCode, reason: &str) {
    let capture = Capture::install();
    let mut stream = quiet(&[answer]);
    assert!(open_through(&mut stream, MAX_DEFERRED_RECHECKS - 1).await);
    assert!(
        ends_quietly_within_a_beat(&mut stream).await,
        "the ceiling ends the stream without access_revoked"
    );
    assert_deferrals(&capture, survivable());
    let closing = capture.only(EVENT_STREAM_UNVERIFIED);
    assert_unanswered(&closing, code, reason);
}

#[tokio::test(start_paused = true)]
async fn should_end_without_access_revoked_when_the_datastore_stays_down() {
    should_end_quietly_at_the_ceiling(Answer::Unreachable, OUTAGE_CODE, OUTAGE_REASON).await;
}

#[tokio::test(start_paused = true)]
async fn should_end_without_access_revoked_when_every_recheck_overruns() {
    should_end_quietly_at_the_ceiling(Answer::Silent, BUDGET_CODE, REASON_BUDGET).await;
}

/// A frame that arrives before the beat goes straight out.
#[tokio::test(start_paused = true)]
async fn should_forward_a_frame_that_arrives_before_the_beat() {
    let frames = stream::iter([Frame::catching_up(1)]).chain(stream::pending());
    let mut stream = watched(frames.boxed(), scripted(&[Answer::Admitted]));
    let first = stream.next().await.expect("the frame goes out");
    assert_eq!(first.kind, afd_sse::KIND_CATCHING_UP);
}
