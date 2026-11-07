//! A report carrying the run's memory: written before the settle commits,
//! fenced on the lease's own token as the settle is, cut at the push's byte
//! cap, and never a reason the answer is lost.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_core::test_util::trace::Capture;
use afd_crypto::entropy::Entropy;
use afd_db::test_util::unreachable_db;
use afd_fleet::lease::Plane;
use afd_memory::Memories;
use afd_wire::memory::{MAX_CONTENT_LEN, MemoryDelta, Visibility};
use afd_wire::report::{Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry};

use crate::report_commit::{RESPONSE_ACCEPTED, RESUME_EVENT_ID, RESUME_RESPONSE};
use crate::report_seed::{Held, SLICE_MS, held};

/// The counter a capture's cut entries are counted on.
const TRUNCATED: &str = "agentsfleet_memory_capture_truncated_total";

/// What the run learned, key and content.
const LEARNED: [(&str, &str); 2] = [("build", "cargo needs two jobs here"), ("owner", "indy")];

/// One delta per key and content, in the core category.
fn deltas<'a>(
    entries: impl IntoIterator<Item = (Cow<'a, str>, Cow<'a, str>)>,
) -> Vec<MemoryDelta<'a>> {
    entries
        .into_iter()
        .map(|(key, content)| MemoryDelta {
            key,
            content,
            category: Cow::Borrowed("core"),
            visibility: Visibility::Fleet,
        })
        .collect()
}

/// [`LEARNED`] as deltas.
fn learned() -> Vec<MemoryDelta<'static>> {
    deltas(LEARNED.map(|(key, content)| (Cow::Borrowed(key), Cow::Borrowed(content))))
}

/// A processed report on `held`'s lease presenting `token`, carrying `memory`.
pub(crate) fn report<'a>(
    held: &'a Held,
    token: u64,
    memory: Vec<MemoryDelta<'a>>,
) -> ReportRequest<'a> {
    ReportRequest {
        task: None,
        lease_id: Cow::Borrowed(held.issued.lease_id.as_str()),
        event_id: Cow::Borrowed(&held.event_id),
        fencing_token: token,
        outcome: Outcome::Processed,
        failure_reason: None,
        failure_detail: Cow::Borrowed(""),
        response_text: Cow::Borrowed(RESPONSE_ACCEPTED),
        tokens: 0,
        requests: afd_wire::usage::RequestLines::default(),
        telemetry: ReportTelemetry {
            time_to_first_token_ms: 0,
            wall_ms: SLICE_MS.unsigned_abs(),
            segments: None,
        },
        checkpoint: ReportCheckpoint {
            last_event_id: Cow::Borrowed(RESUME_EVENT_ID),
            last_response: Cow::Borrowed(RESUME_RESPONSE),
        },
        tool_calls: None,
        held_until: None,
        memory,
    }
}

/// What the fleet's store holds, key and content, sorted.
async fn stored(held: &Held) -> Vec<(String, String)> {
    let fleet = Uuid7::parse(&held.fleet).expect("the fixture id is a v7 spelling");
    let mut window: Vec<(String, String)> =
        Memories::new(held.fixtures.database.clone(), Entropy::new())
            .hydrate(&fleet)
            .await
            .expect("the store answers")
            .memory
            .into_iter()
            .map(|entry| (entry.key.into_owned(), entry.content.into_owned()))
            .collect();
    window.sort();
    window
}

/// [`LEARNED`] as the store holds it.
fn remembered() -> Vec<(String, String)> {
    let mut entries: Vec<(String, String)> = LEARNED
        .iter()
        .map(|(key, content)| ((*key).to_owned(), (*content).to_owned()))
        .collect();
    entries.sort();
    entries
}

/// A report's memory is in the store when the report answers, its
/// completion was published, and the settle answered the instant it did.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_report_memory_lands_before_the_settle() {
    let held = held().await;
    let log = Capture::install();

    let settled = held
        .fixtures
        .plane()
        .report(
            &held.runner,
            &report(&held, held.fence.as_u64(), learned()),
            held.now,
        )
        .await
        .expect("the holder's report settles");
    let published = log.only("completion_published");
    drop(log);

    assert_eq!(
        (
            stored(&held).await,
            held.fixtures.admission_settled(&held.event_id).await,
            settled.completed_at.is_some_and(|at| at >= held.now),
            published.field("lease_id"),
        ),
        (
            remembered(),
            true,
            true,
            Some(held.issued.lease_id.as_str())
        )
    );
    held.fixtures.cleanup().await;
}

/// A lease the fleet has moved past writes neither its memory nor its settle:
/// the capture is fenced on the lease's own token, as the settle is.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_superseded_report_writes_no_memory() {
    let held = held().await;
    held.supersede().await;

    let refused = held
        .fixtures
        .plane()
        .report(
            &held.runner,
            &report(&held, held.fence.as_u64(), learned()),
            held.now,
        )
        .await
        .map(|_settled| ())
        .map_err(|refusal| refusal.code());

    assert_eq!(
        (
            refused,
            stored(&held).await,
            held.fixtures.admission_settled(&held.event_id).await,
        ),
        (Err(error_code::RUN_STALE_FENCING_TOKEN), Vec::new(), false)
    );
    held.fixtures.cleanup().await;
}

/// Memory past the push's byte cap is cut, newest kept, and the report
/// settles: a run's answer is never refused for what it remembered.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_oversize_report_memory_is_cut_and_counted() {
    let held = held().await;
    let metrics = afd_observability::test_util::Capture::install();
    let truncated_before = metrics.sum(TRUNCATED, &[]);
    let content = "m".repeat(MAX_CONTENT_LEN);
    // Twenty full entries, 16 KiB each: fifteen fit the 256 KiB cap.
    let oversize = deltas((0..20).map(|index| {
        (
            Cow::Owned(format!("key-{index:02}")),
            Cow::Borrowed(content.as_str()),
        )
    }));

    held.fixtures
        .plane()
        .report(
            &held.runner,
            &report(&held, held.fence.as_u64(), oversize),
            held.now,
        )
        .await
        .expect("an oversize memory does not refuse the report");

    let kept: Vec<String> = stored(&held)
        .await
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    let first_fifteen: Vec<String> = (0..15).map(|index| format!("key-{index:02}")).collect();
    assert_eq!(
        (
            kept,
            metrics.sum(TRUNCATED, &[]) - truncated_before,
            held.fixtures.admission_settled(&held.event_id).await,
        ),
        (first_fifteen, 5, true),
        "fifteen kept, five counted as cut, the report settled"
    );
    held.fixtures.cleanup().await;
}

/// A memory store that will not answer costs the run its memory, never its
/// answer: the report settles and the failure is logged once.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_failed_capture_still_settles() {
    let held = held().await;
    let forgetful = Plane {
        task_reports: None,
        memories: Memories::new(unreachable_db(), Entropy::new()),
        ..held.fixtures.plane()
    };
    let log = Capture::install();

    forgetful
        .report(
            &held.runner,
            &report(&held, held.fence.as_u64(), learned()),
            held.now,
        )
        .await
        .expect("a failed capture does not refuse the report");
    let failed = log.only("memory_capture_failed");
    drop(log);

    assert_eq!(
        (
            held.fixtures.admission_settled(&held.event_id).await,
            (
                failed
                    .field("error_code")
                    .is_some_and(|code| code.starts_with("UZ-")),
                failed.field("fleet_id"),
                failed.field("lease_id"),
            ),
            stored(&held).await,
        ),
        (
            true,
            (
                true,
                Some(held.fleet.as_str()),
                Some(held.issued.lease_id.as_str())
            ),
            Vec::new()
        )
    );
    drop(forgetful);
    held.fixtures.cleanup().await;
}

/// A repeated report, whose first post wrote the run's memory and settled the
/// lease, skips its capture quietly: the lease is no longer live, the answer
/// is the repeat's, and the store holds the memory once.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_repeated_report_skips_its_memory_capture() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let body = report(&held, held.fence.as_u64(), learned());
    plane
        .report(&held.runner, &body, held.now)
        .await
        .expect("the holder's report settles");
    let log = Capture::install();

    let repeated = plane
        .report(&held.runner, &body, held.now)
        .await
        .expect("a repeated report answers as the first did");
    let skipped = log.only("memory_capture_skipped");
    let failed = log
        .events()
        .iter()
        .filter(|event| event.field("event") == Some("memory_capture_failed"))
        .count();
    drop(log);

    assert_eq!(
        (
            (repeated.repeated, repeated.completed_at),
            (skipped.field("fleet_id"), skipped.field("lease_id")),
            failed,
            stored(&held).await,
        ),
        (
            (true, None),
            (
                Some(held.fleet.as_str()),
                Some(held.issued.lease_id.as_str())
            ),
            0,
            remembered(),
        )
    );
    held.fixtures.cleanup().await;
}
