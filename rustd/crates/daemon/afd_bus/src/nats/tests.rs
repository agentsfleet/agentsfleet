use std::{sync::Arc, time::Instant};

use af_core::id::Uuid7;
use async_nats::Event;

use af_observability::metrics::label::bus::GapCause as Cause;

use super::{Tuning, hub::Channels, options_for, signals, spelling};
use crate::{Error, Received, Subject, error::ErrorKind};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

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
