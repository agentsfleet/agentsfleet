//! The lines the claim loop logs: every failure it continues past, the claim
//! pair, and the wake that stops pinging it.

use std::future::ready;
use std::time::Duration;

use af_core::error_code::{self, Coded as _};
use af_core::test_util::trace::{Capture, CapturedEvent};
use af_crypto::entropy::Entropy;
use afd_bus::Subject;
use tokio_util::sync::CancellationToken;

use super::{
    EVENT_CLAIM_COMPLETED, EVENT_CLAIM_FAILED, EVENT_CLAIM_STARTED, EVENT_WAKE_CLOSED,
    EVENT_WAKE_REOPENED, EVENT_WAKE_SUBSCRIBE_FAILED, Via, Worker, claim_once, pinged, report,
};
use crate::delivery::Outgoing;
use crate::error::query;
use crate::lanes::Lanes;
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
/// finished deliveries alone.
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

/// A wake stream that ends is logged once and dropped, and returns at once so
/// the loop scans and opens the subscription again on its backoff.
#[tokio::test]
async fn a_closed_wake_is_logged_dropped_and_returns() -> TestResult {
    let capture = Capture::install();
    let (bus, _server) = afd_bus::test_util::detached()?;
    let subscription = bus.subscribe(&Subject::DeliveryWake).await?;
    bus.shutdown().await?;
    let mut wake = Some(subscription);

    let waited = tokio::time::timeout(Duration::from_millis(50), pinged(&mut wake)).await;

    assert!(
        waited.is_ok(),
        "a closed wake returns so the loop can reopen it"
    );
    assert!(wake.is_none(), "the closed subscription is dropped");
    assert_unpinged(&capture, EVENT_WAKE_CLOSED);
    let reopened = tokio::time::timeout(Duration::from_millis(50), pinged(&mut wake)).await;
    assert!(reopened.is_err(), "with none open, nothing is reported");
    Ok(())
}

/// No line under `event` was logged; a claim that failed never completes.
fn assert_not_logged(capture: &Capture, event: &str) {
    assert!(
        capture
            .events()
            .iter()
            .all(|record| record.field("event") != Some(event)),
        "nothing was logged under {event}"
    );
}

/// Runs `worker` until it logs `event`, then stops it.
async fn run_until_logged(worker: Worker<Unreached>, capture: &Capture, event: &str) {
    let token = CancellationToken::new();
    let watch = async {
        while capture
            .events()
            .iter()
            .all(|record| record.field("event") != Some(event))
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        token.cancel();
    };
    let stopped = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::join!(worker.run(token.clone()), watch)
    })
    .await;
    assert!(stopped.is_ok(), "the worker never logged {event}");
}

/// A claim Postgres refuses logs its start, takes nothing and never completes.
#[tokio::test]
async fn a_refused_claim_logs_its_start_and_takes_nothing() -> TestResult {
    let capture = Capture::install();
    let database = afd_db::test_util::unreachable_db();
    let lanes = Lanes::new(posters(), database.clone(), CancellationToken::new());

    let claimed = claim_once(&database, &Entropy::new(), &lanes, Via::Ping, lanes.room()).await;

    let failure = claimed.err().ok_or("a refused claim takes nothing")?;
    assert_eq!(failure.code(), error_code::INTERNAL_DB_UNAVAILABLE);
    let started = capture.only(EVENT_CLAIM_STARTED);
    assert_eq!(started.level, tracing::Level::DEBUG);
    assert_eq!(started.field("via"), Some("ping"));
    assert_not_logged(&capture, EVENT_CLAIM_COMPLETED);
    Ok(())
}

/// A claim token the host cannot draw stops the claim before it reaches
/// Postgres, as an operation failure rather than an outage.
#[tokio::test]
async fn a_refused_draw_claims_nothing() -> TestResult {
    let capture = Capture::install();
    let database = afd_db::test_util::unreachable_db();
    let lanes = Lanes::new(posters(), database.clone(), CancellationToken::new());
    let (entropy, control) = Entropy::new_mocked();
    control.fail_next();

    let claimed = claim_once(&database, &entropy, &lanes, Via::Scan, lanes.room()).await;

    let failure = claimed.err().ok_or("a claim with no token takes nothing")?;
    assert_eq!(failure.code(), error_code::INTERNAL_OPERATION_FAILED);
    let reason = failure.logged().reason;
    assert!(
        reason.starts_with("a claim token could not be drawn"),
        "the reason names the draw: {reason}"
    );
    assert_not_logged(&capture, EVENT_CLAIM_COMPLETED);
    Ok(())
}

/// The loop logs a failed claim once at `warn`, under the failure's own code,
/// and claims again on its next ping or finished delivery.
#[tokio::test]
async fn a_failed_claim_is_logged_by_the_loop() -> TestResult {
    let capture = Capture::install();
    let (bus, _server) = afd_bus::test_util::detached()?;
    let worker = Worker::new(
        afd_db::test_util::unreachable_db(),
        bus,
        posters(),
        Entropy::new(),
    );

    run_until_logged(worker, &capture, EVENT_CLAIM_FAILED).await;

    let failed = capture.only(EVENT_CLAIM_FAILED);
    assert_eq!(failed.level, tracing::Level::WARN);
    assert_eq!(
        failed.field("error_code"),
        Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
    );
    failed.assert_uncoded_reason();
    assert_not_logged(&capture, EVENT_CLAIM_COMPLETED);
    Ok(())
}

/// A refused draw reaches the same line, under the operation-failure code.
#[tokio::test]
async fn a_refused_draw_is_logged_by_the_loop() -> TestResult {
    let capture = Capture::install();
    let (bus, _server) = afd_bus::test_util::detached()?;
    let (entropy, control) = Entropy::new_mocked();
    control.fail_next();
    let worker = Worker::new(afd_db::test_util::unreachable_db(), bus, posters(), entropy);

    run_until_logged(worker, &capture, EVENT_CLAIM_FAILED).await;

    let failed = capture.only(EVENT_CLAIM_FAILED);
    assert_eq!(failed.level, tracing::Level::WARN);
    assert_eq!(
        failed.field("error_code"),
        Some(error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
    failed.assert_uncoded_reason();
    Ok(())
}

/// A worker that never got a wake subscription waits on its scans alone: the
/// ping it would have listened for never arrives.
#[tokio::test]
async fn a_worker_with_no_wake_never_reports_a_ping() {
    let mut wake = None;

    let waited = tokio::time::timeout(Duration::from_millis(50), pinged(&mut wake)).await;

    assert!(waited.is_err(), "no subscription, no ping");
    assert!(wake.is_none());
}

/// A bus whose first wake subscription comes from `first`, which the test shuts to
/// close it, and every later one from `then`.
#[derive(Debug)]
struct Swapping {
    first: std::sync::Mutex<Option<afd_bus::Handle>>,
    then: afd_bus::Handle,
}

#[async_trait::async_trait]
impl afd_bus::Bus for Swapping {
    async fn publish(&self, subject: &Subject, payload: &str) -> afd_bus::Result<()> {
        self.then.publish(subject, payload).await
    }

    async fn publish_batch(&self, subject: &Subject, payloads: &[String]) -> afd_bus::Result<()> {
        self.then.publish_batch(subject, payloads).await
    }

    async fn subscribe(&self, subject: &Subject) -> afd_bus::Result<afd_bus::Subscription> {
        let first = self
            .first
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        match first {
            Some(first) => first.subscribe(subject).await,
            None => self.then.subscribe(subject).await,
        }
    }

    async fn subscribe_thread_wakes(&self) -> afd_bus::Result<afd_bus::Subscription> {
        self.then.subscribe_thread_wakes().await
    }

    async fn shutdown(&self) -> afd_bus::Result<()> {
        self.then.shutdown().await
    }
}

/// A wake subscription that closes is opened again on a backoff, and the reopen is
/// logged with its attempt.
#[tokio::test(start_paused = true)]
async fn a_closed_wake_is_opened_again_on_a_backoff() -> TestResult {
    let capture = Capture::install();
    let (first, _first_server) = afd_bus::test_util::detached()?;
    let (then, _then_server) = afd_bus::test_util::detached()?;
    let bus = afd_bus::Handle::new(Swapping {
        first: std::sync::Mutex::new(Some(first.clone())),
        then,
    });
    let worker = Worker::new(
        afd_db::test_util::unreachable_db(),
        bus,
        posters(),
        Entropy::new(),
    );
    let closing = async {
        while capture
            .events()
            .iter()
            .all(|record| record.field("event") != Some(EVENT_CLAIM_STARTED))
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        first.shutdown().await
    };
    let (closed, ()) = tokio::join!(
        closing,
        run_until_logged(worker, &capture, EVENT_WAKE_REOPENED)
    );
    closed?;
    assert_unpinged(&capture, EVENT_WAKE_CLOSED);
    let reopened = capture.only(EVENT_WAKE_REOPENED);
    assert_eq!(reopened.field("attempt"), Some("1"));
    Ok(())
}
