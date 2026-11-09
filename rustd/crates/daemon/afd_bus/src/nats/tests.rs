use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use af_core::{error_code::INTERNAL_OPERATION_FAILED, id::Uuid7, test_util::trace::Capture};
use async_nats::{Client, ConnectOptions, Event, Subscriber};
use futures_util::StreamExt as _;
use tracing::Level;

use af_observability::metrics::label::bus::GapCause as Cause;

use super::{GAP_ALL, Tuning, debt::Owed, hub::Channels, options_for, signals, spelling};
use crate::{Error, Received, Subject, error::ErrorKind};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// A NATS address nothing listens on: port 1 is reserved and unbound.
const NOWHERE: &str = "nats://127.0.0.1:1";

/// How long a test waits for the watcher before it fails.
const BOUND: Duration = Duration::from_secs(10);

/// A fleet's tail subject, as the adapter spells it physically.
const TAIL: &str = "af.fleet.0195b4ba-8d3a-7f13-8abc-2b3e1e0bb011.tail";

const ID: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb011";

#[test]
fn each_subject_has_its_documented_spelling() -> TestResult {
    let id = Uuid7::parse(ID)?;
    assert_eq!(
        spelling(&Subject::FleetTail(id.clone())),
        format!("af.fleet.{ID}.tail")
    );
    assert_eq!(
        spelling(&Subject::DraftEvents(id.clone())),
        format!("af.draft.{ID}.events")
    );
    assert_eq!(spelling(&Subject::Wake(15)), "af.wake.15");
    assert_eq!(spelling(&Subject::OutboundWake), "af.outbound.wake");
    assert_eq!(
        Subject::OutboundWake.kind(),
        "outbound_wake",
        "its telemetry label"
    );
    assert_eq!(
        spelling(&Subject::RunnerCommand(id)),
        format!("af.runner.{ID}.cmd")
    );
    Ok(())
}

#[test]
fn connect_options_never_print_the_password() {
    let pair = ("agentsfleet".to_owned(), "secret".to_owned());
    for options in [options_for(Some(&pair)), options_for(None)] {
        assert!(!format!("{options:?}").contains("secret"));
    }
}

#[test]
fn a_publish_during_an_outage_is_a_backend_failure() {
    let error = Error::from(ErrorKind::NatsDisconnected);
    assert_eq!(error.reason(), "backend");
    assert_eq!(error.code(), af_core::error_code::INTERNAL_OPERATION_FAILED);
    assert!(error.to_string().contains("gap"), "{error}");
}

#[test]
fn a_first_connect_raises_nothing() {
    let signals = signals::Signals::new(&Tuning::default());
    signals.observe(&Event::Connected, Instant::now());
    assert!(signals.raised().is_empty(), "boot is not a reconnect");
}

#[test]
fn a_disconnect_during_the_payment_keeps_publishes_blocked() {
    let signals = signals::Signals::new(&Tuning::default());
    let now = Instant::now();
    signals.observe(&Event::Disconnected, now);
    signals.observe(&Event::Connected, now);
    let before = signals.disconnects();
    // The node drops again while the reconnect's markers are being paid.
    signals.observe(&Event::Disconnected, now);
    assert!(
        !signals.unblock_if_settled(before),
        "the later disconnect wins"
    );
    assert!(signals.blocked(), "publishes still fail fast");
}

#[test]
fn a_settled_reconnect_lifts_the_block_and_an_owed_marker_keeps_it() {
    let signals = signals::Signals::new(&Tuning::default());
    let now = Instant::now();
    signals.observe(&Event::Disconnected, now);
    signals.observe(&Event::Connected, now);
    signals.record("dev.t.af.fleet.a.tail", false, now);
    let before = signals.disconnects();
    assert!(
        !signals.unblock_if_settled(before),
        "a marker is still owed"
    );
    assert!(signals.blocked());
    drop(signals.take_debt());
    assert!(
        signals.unblock_if_settled(before),
        "nothing owed, nothing newer"
    );
    assert!(!signals.blocked());
}

#[test]
fn a_reconnect_and_a_slow_consumer_each_raise_once() {
    let signals = signals::Signals::new(&Tuning::default());
    let now = Instant::now();
    signals.observe(&Event::Disconnected, now);
    signals.observe(&Event::Connected, now);
    signals.observe(&Event::SlowConsumer(7), now);
    signals.observe(&Event::SlowConsumer(9), now);
    signals.observe(&Event::LameDuckMode, now);
    assert_eq!(signals.raised(), [Cause::Reconnect, Cause::SlowConsumer]);
    assert!(signals.raised().is_empty(), "raised causes are consumed");
    signals.observe(&Event::Connected, now);
    assert!(
        signals.raised().is_empty(),
        "a second Connected is no reconnect"
    );
}

#[tokio::test]
async fn a_raised_cause_gaps_every_local_subscription() -> TestResult {
    let channels = Arc::new(Channels::new(8));
    let mut first = channels.open("af.wake.1").reader;
    let mut second = channels.open("af.wake.2").reader;
    assert_eq!(signals::gap(&channels, Cause::Reconnect), 2);
    assert_eq!(first.recv().await?, Received::Gap);
    assert_eq!(second.recv().await?, Received::Gap);
    assert_eq!(signals::gap(&Channels::new(8), Cause::SlowConsumer), 0);
    Ok(())
}

/// A client that dialled [`NOWHERE`] once and gave up, with a subscription it
/// opened before then. The subscription ends once the client has given up,
/// and from then on every publish fails without waiting.
async fn client_that_gave_up() -> TestResult<(Client, Subscriber)> {
    let client = ConnectOptions::new()
        .retry_on_initial_connect()
        .max_reconnects(1)
        .connect(NOWHERE)
        .await?;
    let opened = client.subscribe(GAP_ALL).await?;
    Ok((client, opened))
}

/// [`client_that_gave_up`], once it has.
async fn client_that_has_given_up() -> TestResult<Client> {
    let (client, mut opened) = client_that_gave_up().await?;
    let ended = tokio::time::timeout(BOUND, opened.next()).await?;
    assert!(ended.is_none(), "a client that gave up delivers nothing");
    Ok(client)
}

/// A client still dialling [`NOWHERE`], and a subscription that stays open
/// for as long as it keeps trying.
async fn client_still_dialling() -> TestResult<(Client, Subscriber)> {
    let client = ConnectOptions::new()
        .retry_on_initial_connect()
        .connect(NOWHERE)
        .await?;
    let opened = client.subscribe(GAP_ALL).await?;
    Ok((client, opened))
}

/// The events `capture` holds named `name`.
fn named(capture: &Capture, name: &str) -> usize {
    capture
        .events()
        .iter()
        .filter(|event| event.field("event") == Some(name))
        .count()
}

#[tokio::test]
async fn markers_that_will_not_publish_stay_owed_and_are_reported_once() -> TestResult {
    let client = client_that_has_given_up().await?;
    let signals = signals::Signals::new(&Tuning {
        debt_subjects_max: 2,
        ..Tuning::default()
    });
    let now = Instant::now();
    signals.record(TAIL, false, now);
    // Three recent publishes overflow a debt of two, so the disconnect owes
    // every hub a broadcast marker as well as the failed subject's own.
    for recent in ["af.wake.1", "af.wake.2", "af.wake.3"] {
        signals.record(recent, true, now);
    }
    signals.observe(&Event::Disconnected, now);
    let before = signals.disconnects();
    let capture = Capture::install();

    let paid = signals::pay(&client, &signals, GAP_ALL).await;

    assert_eq!(paid, 0, "nothing reached a client that gave up");
    assert_eq!(named(&capture, "bus_gap_marked"), 0);
    assert_eq!(
        named(&capture, "bus_gap_mark_failed"),
        2,
        "each marker that failed is told once, at debug"
    );
    for failed in capture
        .events()
        .iter()
        .filter(|event| event.field("event") == Some("bus_gap_mark_failed"))
    {
        assert_eq!(failed.level, Level::DEBUG);
        assert_eq!(
            failed.field("error_code"),
            Some(INTERNAL_OPERATION_FAILED.as_str())
        );
        failed.assert_uncoded_reason();
    }
    let owed = capture.only("bus_gap_marks_owed");
    assert_eq!(owed.level, Level::WARN);
    assert_eq!(
        owed.field("error_code"),
        Some(INTERNAL_OPERATION_FAILED.as_str())
    );
    assert_eq!(owed.field("failed"), Some("2"));
    owed.assert_uncoded_reason();
    assert!(
        !signals.unblock_if_settled(before),
        "an unpaid marker keeps publishes failing fast"
    );
    assert_eq!(
        signals.take_debt(),
        Owed {
            subjects: vec![TAIL.to_owned()],
            everyone: true,
        },
        "both markers wait for the next reconnect"
    );
    Ok(())
}

#[tokio::test]
async fn markers_a_reconnect_could_not_pay_are_retried_without_another_gap() -> TestResult {
    let unpayable = client_that_has_given_up().await?;
    let (_dialling, everyone) = client_still_dialling().await?;
    let signals = Arc::new(signals::Signals::new(&Tuning {
        publish_deadline_ms: 10,
        ..Tuning::default()
    }));
    let now = Instant::now();
    signals.observe(&Event::Disconnected, now);
    signals.record(TAIL, false, now);
    signals.observe(&Event::Connected, now);
    let channels = Arc::new(Channels::new(8));
    let mut reader = channels.open(TAIL).reader;
    let capture = Capture::install();

    let watcher = tokio::spawn(signals::watch(
        unpayable,
        Arc::clone(&channels),
        Arc::clone(&signals),
        everyone,
        GAP_ALL.to_owned(),
    ));
    // The reconnect's own payment, then two retries one deadline apart.
    tokio::time::timeout(BOUND, async {
        while named(&capture, "bus_gap_marks_owed") < 3 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    watcher.abort();
    assert!(watcher.await.is_err_and(|stopped| stopped.is_cancelled()));

    assert_eq!(
        reader.recv().await?,
        Received::Gap,
        "the reconnect gaps once"
    );
    assert_eq!(
        named(&capture, "bus_gap_sent"),
        1,
        "a retry pays markers and gaps nobody again"
    );
    assert!(signals.blocked(), "publishes still fail fast");
    assert_eq!(
        signals.take_debt().subjects,
        [TAIL],
        "the marker is still owed"
    );
    Ok(())
}

#[tokio::test]
async fn the_watcher_says_so_and_stops_when_the_broadcast_subscription_ends() -> TestResult {
    let (client, everyone) = client_that_gave_up().await?;
    let channels = Arc::new(Channels::new(8));
    let signals = Arc::new(signals::Signals::new(&Tuning::default()));
    let capture = Capture::install();

    tokio::time::timeout(
        BOUND,
        signals::watch(client, channels, signals, everyone, GAP_ALL.to_owned()),
    )
    .await?;

    let stopped = capture.only("bus_gap_watcher_stopped");
    assert_eq!(stopped.level, Level::WARN);
    assert_eq!(
        stopped.field("error_code"),
        Some(INTERNAL_OPERATION_FAILED.as_str())
    );
    Ok(())
}
