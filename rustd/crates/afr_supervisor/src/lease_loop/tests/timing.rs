//! The terminal line's segments: present and summing to the wall when the
//! lease ran, absent where the lease never reached them.
#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::time::Duration;

use afd_core::test_util::trace::Capture;

use super::super::{EVENT_COMPLETED, EVENT_FAILED};
use crate::client::{Call, Verb};
use crate::test_support::{
    Answer, Behaviour, FLEET_ID, FakeAgent, FakeEngine, LEASE_ID, Rig, daemon, lease,
};

const MEMORY_MS: &str = "memory_ms";
const SANDBOX_MS: &str = "sandbox_ms";
const MODEL_MS: &str = "model_ms";
const TOOLS_MS: &str = "tools_ms";
const REPORT_MS: &str = "report_ms";
const FIRST_TOKEN_MS: &str = "first_token_ms";
/// Six terminal fields, including five sequential time segments.
const FIELDS: [&str; 6] = [
    MEMORY_MS,
    SANDBOX_MS,
    MODEL_MS,
    TOOLS_MS,
    REPORT_MS,
    FIRST_TOKEN_MS,
];
const SUMMED: usize = 5;
const WALL_MS: &str = "wall_ms";
/// The failure's class, on a `lease_failed` line only.
const CLASS: &str = "class";
/// How long the fake agent's run takes before it answers.
const MODEL_TAKES_MS: u64 = 50;
/// Distinct delays for the daemon's answers, so each segment is told apart.
const HYDRATE_TAKES: Duration = Duration::from_millis(40);
const REPORT_TAKES: Duration = Duration::from_millis(30);
/// How far the five segments may fall short of the wall.
const SLACK_MS: u64 = 5;

fn millis(line: &afd_core::test_util::trace::CapturedEvent, field: &str) -> Option<u64> {
    line.field(field).map(|value| value.parse().unwrap())
}

fn ms(duration: Duration) -> Option<u64> {
    u64::try_from(duration.as_millis()).ok()
}

/// The daemon's ordinary answer to `call`, held back by `takes`.
fn late(call: &Call, takes: Duration) -> Answer {
    match daemon(|_call| None)(call) {
        Answer::Reply(bytes) => Answer::Late(takes, bytes),
        other => other,
    }
}

/// A native lease that hydrated, built a sandbox, ran and
/// reported logs every segment, and the six sum to the lease's wall time.
#[tokio::test(start_paused = true)]
async fn test_lease_completed_logs_its_segments() {
    let capture = Capture::install();
    let rig = Rig::new(
        daemon(|_call| None),
        FakeEngine::default(),
        FakeAgent::new(Behaviour::Answer),
    );

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    let line = capture.only(EVENT_COMPLETED);
    let read: Vec<Option<u64>> = FIELDS.iter().map(|field| millis(&line, field)).collect();
    assert!(
        read.iter().all(Option::is_some),
        "every segment is on the line: {:?}",
        line.fields
    );
    let summed: u64 = read.iter().take(SUMMED).flatten().sum();
    let wall = millis(&line, WALL_MS).unwrap();
    assert!(
        wall.abs_diff(summed) <= SLACK_MS,
        "the five segments sum to the wall: {summed} vs {wall} ({:?})",
        line.fields
    );
    let model = millis(&line, MODEL_MS).unwrap();
    assert!(
        millis(&line, FIRST_TOKEN_MS).unwrap() <= model,
        "the first token is inside the model's time"
    );
}

/// Each segment times its own step: a late hydrate is memory time, the agent's run is model time with no tool time,
/// and a late report is report time, so a stopwatch in the wrong place shows.
#[tokio::test(start_paused = true)]
async fn test_each_segment_times_its_own_step() {
    let capture = Capture::install();
    let rig = Rig::new(
        daemon(move |call| match call.verb {
            Verb::Hydrate => Some(late(call, HYDRATE_TAKES)),
            Verb::Report => Some(late(call, REPORT_TAKES)),
            _ => None,
        }),
        FakeEngine::default(),
        FakeAgent::new(Behaviour::Answer),
    );

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    let line = capture.only(EVENT_COMPLETED);
    assert_eq!(
        millis(&line, MEMORY_MS),
        ms(HYDRATE_TAKES),
        "the late hydrate is memory time: {:?}",
        line.fields
    );
    assert_eq!(
        millis(&line, MODEL_MS),
        Some(MODEL_TAKES_MS),
        "setup is not model time"
    );
    assert_eq!(
        millis(&line, TOOLS_MS),
        Some(0),
        "a run that called no tool"
    );
    assert!(
        millis(&line, REPORT_MS).unwrap() >= ms(REPORT_TAKES).unwrap(),
        "the report post is report time: {:?}",
        line.fields
    );
    let summed: u64 = FIELDS
        .iter()
        .take(SUMMED)
        .filter_map(|field| millis(&line, field))
        .sum();
    let wall = millis(&line, WALL_MS).unwrap();
    assert!(wall.abs_diff(summed) <= SLACK_MS, "{summed} vs {wall}");
    assert_eq!(line.field(CLASS), None, "a completion carries no class");
}

/// A sandbox the engine refuses still leaves its segment on the line: the
/// stopwatch records on the refusal exit, and nothing after it is logged.
#[tokio::test(start_paused = true)]
async fn test_a_refused_sandbox_still_logs_its_segment() {
    let capture = Capture::install();
    let rig = Rig::new(
        daemon(|_call| None),
        FakeEngine {
            refuse: true,
            ..FakeEngine::default()
        },
        FakeAgent::new(Behaviour::Answer),
    );

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    let line = capture.only(EVENT_FAILED);
    for present in [MEMORY_MS, SANDBOX_MS, REPORT_MS, CLASS] {
        assert!(
            line.field(present).is_some(),
            "{present}: {:?}",
            line.fields
        );
    }
    for absent in [MODEL_MS, TOOLS_MS, FIRST_TOKEN_MS] {
        assert_eq!(line.field(absent), None, "{absent}: {:?}", line.fields);
    }
}

/// A lease refused before its sandbox was sized logs what ran and leaves
/// what never started off the line, rather than at zero.
#[tokio::test(start_paused = true)]
async fn test_unstarted_segment_is_absent() {
    let capture = Capture::install();
    let rig = Rig::new(
        daemon(|call| {
            (call.verb == Verb::Hydrate)
                .then(|| Answer::Fail(crate::error::refused(Verb::Hydrate, 503, None)))
        }),
        FakeEngine::default(),
        FakeAgent::new(Behaviour::Answer),
    );

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    let line = capture.only(EVENT_FAILED);
    assert!(
        millis(&line, MEMORY_MS).is_some(),
        "the hydrate ran: {:?}",
        line.fields
    );
    assert!(millis(&line, REPORT_MS).is_some(), "the report settled");
    assert!(line.field(CLASS).is_some(), "a failure names its class");
    for absent in [SANDBOX_MS, MODEL_MS, TOOLS_MS, FIRST_TOKEN_MS] {
        assert_eq!(
            line.field(absent),
            None,
            "{absent} never started, so it is absent, not zero: {:?}",
            line.fields
        );
    }
}
