//! A restarted replica asks for one pass when the last outstanding run lapses,
//! and none when its read finds no such run or fails.

use std::{sync::Arc, time::Duration};

use af_core::clock::{self, FixedClock};
use afd_bus::wake::{Generation, Wake};
use jiff::Timestamp;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::boot;
use crate::error::ErrorKind;

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

/// The wall-clock instant every replica here boots at.
const NOW: Timestamp = Timestamp::constant(1_790_000_000, 0);

/// The finest step a deadline takes in the column and on the wire.
const ONE_MILLISECOND: Duration = Duration::from_millis(1);

fn gateway(wake: &Wake) -> Result<crate::Gateway, Box<dyn std::error::Error + Send + Sync>> {
    crate::fixture::clocked(
        afd_db::test_util::unreachable_db(),
        afd_bus::test_util::unreachable(),
        wake.clone(),
        Arc::new(FixedClock::at(NOW)),
    )
}

/// Whether a pass was requested since the last look; marks it seen.
fn requested(
    generations: &mut watch::Receiver<Generation>,
) -> Result<bool, watch::error::RecvError> {
    let changed = generations.has_changed()?;
    generations.borrow_and_update();
    Ok(changed)
}

/// Boots with `read` as the outstanding-run read, until `stop`.
fn booted(
    gateway: &crate::Gateway,
    stop: &CancellationToken,
    read: crate::Result<Option<Timestamp>>,
) -> tokio::task::JoinHandle<()> {
    let (gateway, stop) = (gateway.clone(), stop.clone());
    tokio::spawn(async move { boot(&gateway.inner.dependencies, &stop, async { read }).await })
}

async fn advance(span: Duration) {
    tokio::time::advance(span).await;
    tokio::task::yield_now().await;
}

#[tokio::test(start_paused = true)]
async fn a_boot_with_an_outstanding_run_requests_one_pass_when_it_lapses() -> TestResult {
    let wake = Wake::default();
    let gateway = gateway(&wake)?;
    let stop = CancellationToken::new();
    let mut generations = wake.generations();
    generations.borrow_and_update();

    let boot = booted(
        &gateway,
        &stop,
        Ok(Some(clock::after(NOW, Duration::from_secs(30)))),
    );
    tokio::task::yield_now().await;
    advance(Duration::from_millis(29_999)).await;
    assert!(!requested(&mut generations)?, "no pass before the lapse");
    advance(ONE_MILLISECOND).await;
    assert!(requested(&mut generations)?, "one pass at the lapse");
    assert!(wake.recovery().is_some(), "the pass is a full recheck");
    assert!(boot.is_finished(), "the boot read never repeats");
    advance(Duration::from_secs(600)).await;
    assert!(!requested(&mut generations)?);
    boot.await?;
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_boot_with_no_outstanding_run_requests_no_pass() -> TestResult {
    let wake = Wake::default();
    let gateway = gateway(&wake)?;
    let stop = CancellationToken::new();
    let mut generations = wake.generations();
    generations.borrow_and_update();

    booted(&gateway, &stop, Ok(None)).await?;
    advance(Duration::from_secs(600)).await;
    assert!(!requested(&mut generations)?);
    assert!(wake.recovery().is_none());
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_failed_boot_read_is_observed_once_and_requests_no_pass() -> TestResult {
    let wake = Wake::default();
    let gateway = gateway(&wake)?;
    let stop = CancellationToken::new();
    let mut generations = wake.generations();
    generations.borrow_and_update();

    let capture = af_core::test_util::trace::Capture::install();
    booted(&gateway, &stop, Err(ErrorKind::Closed.into())).await?;
    let failed = capture.only("runner_stream_operation_failed");
    assert_eq!(failed.field("operation"), Some("recheck"));
    assert_eq!(
        failed.field("error_code"),
        Some(crate::Error::from(ErrorKind::Closed).code().as_str())
    );
    drop(capture);
    advance(Duration::from_secs(600)).await;
    assert!(!requested(&mut generations)?);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_stop_before_the_lapse_ends_the_wait_without_a_pass() -> TestResult {
    let wake = Wake::default();
    let gateway = gateway(&wake)?;
    let stop = CancellationToken::new();
    let mut generations = wake.generations();
    generations.borrow_and_update();

    let boot = booted(
        &gateway,
        &stop,
        Ok(Some(clock::after(NOW, Duration::from_secs(30)))),
    );
    tokio::task::yield_now().await;
    advance(Duration::from_secs(10)).await;
    stop.cancel();
    boot.await?;
    advance(Duration::from_secs(600)).await;
    assert!(!requested(&mut generations)?);
    Ok(())
}
