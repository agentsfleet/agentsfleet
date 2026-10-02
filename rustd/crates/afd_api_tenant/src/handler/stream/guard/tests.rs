//! The fleet stream's re-check, against a question answered on demand: each
//! arm of the answer, on a paused clock, with what it logged.
#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use std::time::Duration;

use afd_core::error_code::{self, ErrorCode};
use afd_core::id::Uuid7;
use afd_core::test_util::trace::Capture;
use afd_http::handler::Refusable;
use afd_sse::{Frame, KIND_ACCESS_REVOKED};
use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};

use super::{
    EVENT_RECHECK_DEFERRED, REASON_BUDGET, RECHECK_BUDGET, RECHECK_INTERVAL, Recheck, watched,
};

const WORKSPACE: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1011";

/// What the fixture outage says about itself.
const OUTAGE_REASON: &str = "the pool timed out";

/// Past one re-check and its budget, short of the next re-check.
const ONE_BEAT: Duration = RECHECK_INTERVAL.saturating_add(RECHECK_BUDGET.saturating_mul(2));

/// A datastore that would not answer.
#[derive(Debug)]
struct Outage;

impl Refusable for Outage {
    fn code(&self) -> ErrorCode {
        error_code::INTERNAL_DB_UNAVAILABLE
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

/// How the fixture question answers.
#[derive(Debug, Clone, Copy)]
enum Answer {
    Admitted,
    Revoked,
    Unreachable,
    Silent,
}

struct Asked {
    answer: Answer,
    workspace: Uuid7,
}

impl Recheck for Asked {
    type Error = Outage;

    async fn admitted(&self) -> Result<bool, Outage> {
        match self.answer {
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

/// The workspace every guarded stream here belongs to.
fn workspace() -> Uuid7 {
    Uuid7::parse(WORKSPACE).expect("the fixture identifier is UUIDv7")
}

/// A stream with no frames of its own, guarded by a question that answers
/// `answer`.
fn quiet(answer: Answer) -> BoxStream<'static, Frame> {
    let workspace = workspace();
    watched(stream::pending().boxed(), Asked { answer, workspace })
}

/// Whether the stream is still open after one beat: nothing sent, not ended.
async fn open_after_one_beat(stream: &mut BoxStream<'static, Frame>) -> bool {
    tokio::time::timeout(ONE_BEAT, stream.next()).await.is_err()
}

/// The one deferred record, checked for its code and for anything that names
/// a person.
fn the_deferral(capture: &Capture, code: ErrorCode, reason: &str) {
    let deferred = capture.only(EVENT_RECHECK_DEFERRED);
    assert_eq!(deferred.level, tracing::Level::WARN);
    let field = |name: &str| deferred.fields.get(name).map(String::as_str);
    assert_eq!(field("error_code"), Some(code.as_str()));
    assert_eq!(field("workspace_id"), Some(WORKSPACE));
    assert_eq!(field("reason"), Some(reason));
    assert!(
        deferred.fields.values().all(|value| !value.contains('@')),
        "no address in the record: {deferred:?}"
    );
}

/// A datastore that will not answer keeps the stream, and the deferral is
/// logged once with the outage's own code.
#[tokio::test(start_paused = true)]
async fn should_keep_the_stream_and_log_once_when_the_recheck_fails() {
    let capture = Capture::install();
    let mut stream = quiet(Answer::Unreachable);
    assert!(open_after_one_beat(&mut stream).await);
    the_deferral(&capture, error_code::INTERNAL_DB_UNAVAILABLE, OUTAGE_REASON);
}

/// A re-check past its budget keeps the stream, and the deferral is logged
/// once as the datastore not answering in time.
#[tokio::test(start_paused = true)]
async fn should_keep_the_stream_and_log_once_when_the_recheck_overruns_its_budget() {
    let capture = Capture::install();
    let mut stream = quiet(Answer::Silent);
    assert!(open_after_one_beat(&mut stream).await);
    the_deferral(&capture, error_code::INTERNAL_DB_UNAVAILABLE, REASON_BUDGET);
}

/// A caller still admitted keeps the stream and nothing is deferred.
#[tokio::test(start_paused = true)]
async fn should_keep_the_stream_quietly_while_the_caller_is_admitted() {
    let capture = Capture::install();
    let mut stream = quiet(Answer::Admitted);
    assert!(open_after_one_beat(&mut stream).await);
    assert!(
        capture.events().iter().all(|event| {
            event.fields.get("event").map(String::as_str) != Some(EVENT_RECHECK_DEFERRED)
        }),
        "an answered re-check defers nothing"
    );
}

/// A caller who lost the workspace gets `access_revoked`, then the end.
#[tokio::test(start_paused = true)]
async fn should_end_on_access_revoked_once_the_caller_is_refused() {
    let mut stream = quiet(Answer::Revoked);
    let last = stream.next().await.expect("the revocation is sent");
    assert_eq!(last.kind, KIND_ACCESS_REVOKED);
    assert!(stream.next().await.is_none(), "nothing follows it");
}

/// A frame that arrives before the beat goes straight out.
#[tokio::test(start_paused = true)]
async fn should_forward_a_frame_that_arrives_before_the_beat() {
    let workspace = workspace();
    let frames = stream::iter([Frame::catching_up(1)]).chain(stream::pending());
    let mut stream = watched(
        frames.boxed(),
        Asked {
            answer: Answer::Admitted,
            workspace,
        },
    );
    let first = stream.next().await.expect("the frame goes out");
    assert_eq!(first.kind, afd_sse::KIND_CATCHING_UP);
}
