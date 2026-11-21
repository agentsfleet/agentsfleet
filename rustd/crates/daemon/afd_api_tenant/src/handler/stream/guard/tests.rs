//! The fleet stream's re-check, against a question that answers from a
//! script: each arm of the answer, and what each logged, on a paused clock.
#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use af_core::error_code::{self, ErrorCode};
use af_core::id::Uuid7;
use af_core::test_util::trace::{Capture, CapturedEvent};
use afd_http::handler::Refusable;
use afd_sse::{Frame, KIND_ACCESS_REVOKED};
use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};

use super::super::revocable::{EVENT_STREAM_UNVERIFIED, STREAM_FLEET};
use super::{RECHECK_INTERVAL, Recheck, watched};

const WORKSPACE: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1011";

/// What the fixture outage says about itself.
const OUTAGE_REASON: &str = "the pool timed out";

/// The fixture outage's code.
const OUTAGE_CODE: ErrorCode = error_code::INTERNAL_DB_QUERY;

/// Half a beat: past one re-check, short of the next.
const HALF_BEAT: Duration = RECHECK_INTERVAL.checked_div(2).expect("a beat halves");

/// Past one re-check, short of the next.
const ONE_BEAT: Duration = RECHECK_INTERVAL.saturating_add(HALF_BEAT);

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

    fn admitted(&self) -> impl Future<Output = Result<bool, Outage>> + Send {
        let turn = self.asked.fetch_add(1, Ordering::Relaxed);
        let answer = self.answers.get(turn).or_else(|| self.answers.last());
        std::future::ready(match answer.expect("every script has an answer") {
            Answer::Admitted => Ok(true),
            Answer::Revoked => Ok(false),
            Answer::Unreachable => Err(Outage),
        })
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
        .saturating_add(HALF_BEAT);
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

/// Every record raised under `event`, in order.
fn records(capture: &Capture, event: &str) -> Vec<CapturedEvent> {
    let mut all = capture.events();
    all.retain(|record| record.field("event") == Some(event));
    all
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

/// A datastore that will not answer ends the stream with no frame — an
/// outage is not a revocation — and the closing is logged once with the
/// outage's own code.
#[tokio::test(start_paused = true)]
async fn should_end_quietly_and_log_once_when_the_recheck_fails() {
    let capture = Capture::install();
    let mut stream = quiet(&[Answer::Unreachable]);
    assert!(
        ends_quietly_within_a_beat(&mut stream).await,
        "an unanswered check ends the stream without access_revoked"
    );
    let closing = capture.only(EVENT_STREAM_UNVERIFIED);
    assert_unanswered(&closing, OUTAGE_CODE, OUTAGE_REASON);
}

/// A caller still admitted keeps the stream, and nothing is closed.
#[tokio::test(start_paused = true)]
async fn should_keep_the_stream_quietly_while_the_caller_is_admitted() {
    let capture = Capture::install();
    let mut stream = quiet(&[Answer::Admitted]);
    assert!(open_through(&mut stream, 3).await);
    assert!(records(&capture, EVENT_STREAM_UNVERIFIED).is_empty());
}

/// A caller who lost the workspace gets `access_revoked`, then the end.
#[tokio::test(start_paused = true)]
async fn should_end_on_access_revoked_once_the_caller_is_refused() {
    let mut stream = quiet(&[Answer::Revoked]);
    assert_ends_on_access_revoked(&mut stream).await;
}

/// A revocation after admitted beats lands on its own beat.
#[tokio::test(start_paused = true)]
async fn should_end_on_access_revoked_on_the_beat_it_lands() {
    let mut stream = quiet(&[Answer::Admitted, Answer::Revoked]);
    assert!(
        open_through(&mut stream, 1).await,
        "the admitted beat keeps it"
    );
    assert_ends_on_access_revoked(&mut stream).await;
}

/// A frame that arrives before the beat goes straight out.
#[tokio::test(start_paused = true)]
async fn should_forward_a_frame_that_arrives_before_the_beat() {
    let frames = stream::iter([Frame::catching_up(1)]).chain(stream::pending());
    let mut stream = watched(frames.boxed(), scripted(&[Answer::Admitted]));
    let first = stream.next().await.expect("the frame goes out");
    assert_eq!(first.kind, afd_sse::KIND_CATCHING_UP);
}
