//! The terminal line's segments: present and summing to the wall when the
//! lease ran, absent where the lease never reached them.
#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use afd_core::bundle::BundleDigest;
use afd_core::test_util::trace::Capture;
use bytes::Bytes;

use crate::client::Verb;
use crate::test_support::{
    Answer, Behaviour, FLEET_ID, FakeAgent, FakeEngine, LEASE_ID, Rig, daemon, lease,
};

/// The bundle's instructions, the one file the fake bundle holds.
const SKILL: &[u8] = b"skill";
const BUNDLE_MS: &str = "bundle_ms";
const MEMORY_MS: &str = "memory_ms";
const SANDBOX_MS: &str = "sandbox_ms";
const MODEL_MS: &str = "model_ms";
const TOOLS_MS: &str = "tools_ms";
const REPORT_MS: &str = "report_ms";
const FIRST_TOKEN_MS: &str = "first_token_ms";
/// The seven fields a terminal line carries, and the six that sum.
const FIELDS: [&str; 7] = [
    BUNDLE_MS,
    MEMORY_MS,
    SANDBOX_MS,
    MODEL_MS,
    TOOLS_MS,
    REPORT_MS,
    FIRST_TOKEN_MS,
];
const SUMMED: usize = 6;
const WALL_MS: &str = "wall_ms";
const EVENT_COMPLETED: &str = "lease_completed";
const EVENT_FAILED: &str = "lease_failed";
/// How far the six segments may fall short of the wall.
const SLACK_MS: u64 = 5;

/// A one-file bundle and the name the importer gives it.
fn bundle() -> (Bytes, String) {
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(u64::try_from(SKILL.len()).unwrap());
    header.set_mode(0o644);
    header.set_cksum();
    builder.append_data(&mut header, "SKILL.md", SKILL).unwrap();
    let digest = BundleDigest::new(SKILL, None);
    (Bytes::from(builder.into_inner().unwrap()), digest.finish())
}

fn millis(line: &afd_core::test_util::trace::CapturedEvent, field: &str) -> Option<u64> {
    line.field(field).map(|value| value.parse().unwrap())
}

/// A lease that fetched a bundle, hydrated, built a sandbox, ran and
/// reported logs every segment, and the six sum to the lease's wall time.
#[tokio::test(start_paused = true)]
async fn test_lease_completed_logs_its_segments() {
    let capture = Capture::install();
    let (archive, name) = bundle();
    let rig = Rig::new(
        daemon(move |call| (call.verb == Verb::Bundle).then(|| Answer::Reply(archive.clone()))),
        FakeEngine::default(),
        FakeAgent::new(Behaviour::Answer),
    );

    rig.run(&lease(LEASE_ID, FLEET_ID, Some(&name)))
        .await
        .unwrap();

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
        "the six segments sum to the wall: {summed} vs {wall} ({:?})",
        line.fields
    );
    let model = millis(&line, MODEL_MS).unwrap();
    assert!(
        millis(&line, FIRST_TOKEN_MS).unwrap() <= model,
        "the first token is inside the model's time"
    );
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

    rig.run(&lease(LEASE_ID, FLEET_ID, None)).await.unwrap();

    let line = capture.only(EVENT_FAILED);
    assert!(
        millis(&line, MEMORY_MS).is_some(),
        "the hydrate ran: {:?}",
        line.fields
    );
    assert!(millis(&line, REPORT_MS).is_some(), "the report settled");
    for absent in [BUNDLE_MS, SANDBOX_MS, MODEL_MS, TOOLS_MS, FIRST_TOKEN_MS] {
        assert_eq!(
            line.field(absent),
            None,
            "{absent} never started, so it is absent, not zero: {:?}",
            line.fields
        );
    }
}
