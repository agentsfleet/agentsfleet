//! The fleet stream's re-check, against a question that answers from a
//! script and notices a test sends: each arm of the answer, what each logged,
//! and that nothing is asked without a notice, on a paused clock.
#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use af_core::error_code::{self, ErrorCode};
use af_core::id::Uuid7;
use af_core::test_util::trace::{Capture, CapturedEvent};
use afd_http::handler::Refusable;
use afd_sse::{Frame, KIND_ACCESS_REVOKED, Wake};
use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};

use super::super::revocable::{EVENT_STREAM_UNVERIFIED, STREAM_FLEET};
use super::{Recheck, watched};

const WORKSPACE: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1011";

/// What the fixture outage says about itself.
const OUTAGE_REASON: &str = "the pool timed out";

/// The fixture outage's code.
const OUTAGE_CODE: ErrorCode = error_code::INTERNAL_DB_QUERY;

/// Longer than any re-check takes on the paused clock: an hour of idle time.
const AN_HOUR: Duration = Duration::from_secs(3600);

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

/// How the fixture question answers one re-check.
#[derive(Debug, Clone, Copy)]
enum Answer {
    Admitted,
    Revoked,
    Unreachable,
}

/// A question answering from a script, one answer per re-check; past its end it
/// repeats the last. `asked` indexes the script, because `admitted` takes
/// `&self`, and the test reads it to count the re-checks.
struct Scripted {
    answers: Vec<Answer>,
    asked: Arc<AtomicUsize>,
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

/// A question answering `answers` in turn, counting into `asked`.
fn scripted(answers: &[Answer], asked: &Arc<AtomicUsize>) -> Scripted {
    Scripted {
        answers: answers.to_vec(),
        asked: Arc::clone(asked),
        workspace: Uuid7::parse(WORKSPACE).expect("the fixture identifier is UUIDv7"),
    }
}

/// A stream with frames `frames`, guarded by a question answering `answers` in
/// turn, with the sender of its notices and the count of re-checks asked.
struct Guarded {
    stream: BoxStream<'static, Frame>,
    notices: tokio::sync::mpsc::UnboundedSender<Wake>,
    asked: Arc<AtomicUsize>,
}

fn guarded_with(frames: BoxStream<'static, Frame>, answers: &[Answer]) -> Guarded {
    let asked = Arc::new(AtomicUsize::new(0));
    let (notices, received) = tokio::sync::mpsc::unbounded_channel();
    let received = stream::unfold(received, async |mut received| {
        received.recv().await.map(|wake| (wake, received))
    });
    Guarded {
        stream: watched(frames, scripted(answers, &asked), received.boxed()),
        notices,
        asked,
    }
}

/// A stream with no frames of its own.
fn quiet(answers: &[Answer]) -> Guarded {
    guarded_with(stream::pending().boxed(), answers)
}

impl Guarded {
    /// Sends one notice that the account's members may have changed.
    fn ring(&self, wake: Wake) {
        self.notices
            .send(wake)
            .expect("the guard holds the receiver");
    }

    /// How many times the guard asked.
    fn asked(&self) -> usize {
        self.asked.load(Ordering::Relaxed)
    }

    /// Whether the stream stays open through an hour of idle time: nothing sent,
    /// not ended.
    async fn open_for_an_hour(&mut self) -> bool {
        tokio::time::timeout(AN_HOUR, self.stream.next())
            .await
            .is_err()
    }

    /// Whether the stream ends without sending anything, `access_revoked` included.
    async fn ends_quietly(&mut self) -> bool {
        matches!(
            tokio::time::timeout(AN_HOUR, self.stream.next()).await,
            Ok(None)
        )
    }

    /// The stream's next frame is `access_revoked`, and nothing follows it.
    async fn assert_ends_on_access_revoked(&mut self) {
        let last = tokio::time::timeout(AN_HOUR, self.stream.next())
            .await
            .expect("the notice is answered")
            .expect("the revocation is sent");
        assert_eq!(last.kind, KIND_ACCESS_REVOKED);
        assert!(self.stream.next().await.is_none(), "nothing follows it");
    }
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

/// No notice, no question: an idle stream asks nothing however long it stays
/// open, which is the timer this guard no longer runs.
#[tokio::test(start_paused = true)]
async fn an_idle_stream_asks_nothing_without_a_notice() {
    let mut guarded = quiet(&[Answer::Revoked]);
    assert!(guarded.open_for_an_hour().await);
    assert_eq!(guarded.asked(), 0);
}

/// A datastore that will not answer ends the stream with no frame — an
/// outage is not a revocation — and the closing is logged once with the
/// outage's own code.
#[tokio::test(start_paused = true)]
async fn should_end_quietly_and_log_once_when_the_recheck_fails() {
    let capture = Capture::install();
    let mut guarded = quiet(&[Answer::Unreachable]);
    guarded.ring(Wake::Changed);
    assert!(
        guarded.ends_quietly().await,
        "an unanswered check ends the stream without access_revoked"
    );
    let closing = capture.only(EVENT_STREAM_UNVERIFIED);
    assert_unanswered(&closing, OUTAGE_CODE, OUTAGE_REASON);
}

/// A caller still admitted keeps the stream through every notice, and nothing
/// is closed.
#[tokio::test(start_paused = true)]
async fn should_keep_the_stream_quietly_while_the_caller_is_admitted() {
    let capture = Capture::install();
    let mut guarded = quiet(&[Answer::Admitted]);
    for _ in 0..3 {
        guarded.ring(Wake::Changed);
    }
    assert!(guarded.open_for_an_hour().await);
    assert_eq!(guarded.asked(), 3, "one question per notice");
    assert!(records(&capture, EVENT_STREAM_UNVERIFIED).is_empty());
}

/// A caller who lost the workspace gets `access_revoked` on the next notice,
/// then the end.
#[tokio::test(start_paused = true)]
async fn should_end_on_access_revoked_once_the_caller_is_refused() {
    let mut guarded = quiet(&[Answer::Revoked]);
    guarded.ring(Wake::Changed);
    guarded.assert_ends_on_access_revoked().await;
}

/// A revocation after admitted notices lands on its own notice.
#[tokio::test(start_paused = true)]
async fn should_end_on_access_revoked_on_the_notice_it_lands() {
    let mut guarded = quiet(&[Answer::Admitted, Answer::Revoked]);
    guarded.ring(Wake::Changed);
    assert!(
        guarded.open_for_an_hour().await,
        "the admitted notice keeps it"
    );
    guarded.ring(Wake::Changed);
    guarded.assert_ends_on_access_revoked().await;
}

/// A bus that shut down ends the stream with no frame and no question, so the
/// client reconnects to a serving replica.
#[tokio::test(start_paused = true)]
async fn a_closed_bus_ends_the_stream_without_asking() {
    let mut guarded = quiet(&[Answer::Revoked]);
    guarded.ring(Wake::Closed);
    assert!(guarded.ends_quietly().await);
    assert_eq!(guarded.asked(), 0);
}

/// Notices that stop for good end the stream the same way.
#[tokio::test(start_paused = true)]
async fn notices_that_end_end_the_stream() {
    let Guarded {
        mut stream,
        notices,
        ..
    } = quiet(&[Answer::Admitted]);
    drop(notices);
    assert!(matches!(
        tokio::time::timeout(AN_HOUR, stream.next()).await,
        Ok(None)
    ));
}

/// A frame that arrives goes straight out, with no question asked.
#[tokio::test(start_paused = true)]
async fn should_forward_a_frame_without_asking() {
    let frames = stream::iter([Frame::catching_up(1)]).chain(stream::pending());
    let mut guarded = guarded_with(frames.boxed(), &[Answer::Admitted]);
    let first = guarded.stream.next().await.expect("the frame goes out");
    assert_eq!(first.kind, afd_sse::KIND_CATCHING_UP);
    assert_eq!(guarded.asked(), 0);
}
