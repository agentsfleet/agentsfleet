//! The lines the claim loop logs: every failure it continues past, the claim
//! pair, and the wake that stops pinging it.

use std::future::ready;
use std::time::Duration;

use af_core::error_code;
use af_core::test_util::trace::{Capture, CapturedEvent};
use af_crypto::entropy::Entropy;
use afd_bus::Subject;
use tokio_util::sync::CancellationToken;

use super::{
    EVENT_CLAIM_COMPLETED, EVENT_CLAIM_FAILED, EVENT_CLAIM_STARTED, EVENT_WAKE_CLOSED,
    EVENT_WAKE_SUBSCRIBE_FAILED, Via, Worker, claim_once, pinged, report,
};
use crate::error::query;
use crate::lanes::Lanes;
use crate::obligation::Outgoing;
use crate::poster::{Deliver, Posters, Verdict};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

/// The event the fixture failure is reported under.
const EVENT: &str = "outbound_fixture_failed";

/// A poster nothing here reaches: no claim in these tests takes a row.
#[derive(Debug)]
struct Unreached;

impl Deliver for Unreached {
    fn deliver(&self, _job: &Outgoing) -> impl Future<Output = Verdict> + Send {
        ready(Verdict::Delivered)
    }
}

const fn posters() -> Posters<Unreached> {
    Posters { slack: Unreached }
}

/// The one warning a lost wake leaves under `event`: the bus failure's own
/// registry code, and its reason without that code a second time.
fn assert_unpinged(capture: &Capture, event: &str) -> CapturedEvent {
    let logged = capture.only(event);
    assert_eq!(logged.level, tracing::Level::WARN);
    let code = logged.field("error_code").unwrap_or_default();
    assert!(
        error_code::ErrorCode::lookup(code).is_some(),
        "the warning carries a registry code: {logged:?}"
    );
    logged.assert_uncoded_reason();
    logged
}

/// The line carries the registry code in its own field, so the `reason` beside
/// it is the failure's sentence and its causes — never the code a second time,
/// and never a captured backtrace.
#[test]
fn a_logged_failure_reason_carries_no_code_prefix() {
    let capture = Capture::install();
    let failure = query("recording the fixture")(sqlx::Error::PoolClosed);

    report(EVENT, &failure);

    let logged = capture.only(EVENT);
    assert_eq!(logged.field("error_code"), Some(failure.code().as_str()));
    logged.assert_uncoded_reason();
    let reason = logged.field("reason").unwrap_or_default();
    assert!(
        reason.starts_with("statement failed during recording the fixture"),
        "the reason opens with the failure's own sentence: {reason}"
    );
}

/// A worker whose bus refuses the subscription says so once and runs on its
/// pass alone.
#[tokio::test]
async fn a_wake_that_cannot_subscribe_is_logged() -> TestResult {
    let capture = Capture::install();
    let (bus, _server) = afd_bus::test_util::detached()?;
    bus.shutdown().await?;
    let stopped = CancellationToken::new();
    stopped.cancel();

    let database = afd_db::test_util::unreachable_db();
    Worker::new(database, bus, posters(), Entropy::new())
        .run(stopped)
        .await;

    let logged = assert_unpinged(&capture, EVENT_WAKE_SUBSCRIBE_FAILED);
    assert_eq!(
        logged.field("error_code"),
        Some(error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
    assert_eq!(
        logged.field("reason"),
        Some("closed"),
        "a shut bus refuses as closed"
    );
    Ok(())
}

/// A wake stream that ends is logged once and dropped, and never reports a
/// ping after it.
#[tokio::test]
async fn a_closed_wake_is_logged_and_dropped() -> TestResult {
    let capture = Capture::install();
    let (bus, _server) = afd_bus::test_util::detached()?;
    let subscription = bus.subscribe(&Subject::OutboundWake).await?;
    bus.shutdown().await?;
    let mut wake = Some(subscription);

    let waited = tokio::time::timeout(Duration::from_millis(50), pinged(&mut wake)).await;

    assert!(waited.is_err(), "a closed wake never reports a ping");
    assert!(wake.is_none(), "the worker stops listening");
    assert_unpinged(&capture, EVENT_WAKE_CLOSED);
    Ok(())
}

/// A claim Postgres refuses logs its start and its failure, and no completion.
#[tokio::test]
async fn a_refused_claim_logs_its_start_and_its_failure() {
    let capture = Capture::install();
    let database = afd_db::test_util::unreachable_db();
    let lanes = Lanes::new(posters(), database.clone(), CancellationToken::new());

    let claimed = claim_once(&database, &Entropy::new(), &lanes, Via::Ping).await;

    assert_eq!(claimed, None, "a refused claim takes nothing");
    let started = capture.only(EVENT_CLAIM_STARTED);
    assert_eq!(started.level, tracing::Level::DEBUG);
    assert_eq!(started.field("via"), Some("ping"));
    let failed = capture.only(EVENT_CLAIM_FAILED);
    assert_eq!(failed.level, tracing::Level::WARN);
    assert_eq!(
        failed.field("error_code"),
        Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
    );
    assert!(
        capture
            .events()
            .iter()
            .all(|record| record.field("event") != Some(EVENT_CLAIM_COMPLETED)),
        "a refused claim never completes"
    );
}
