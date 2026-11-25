//! A departed connection asks for one pass at the last lapse of the runs it
//! still held, and only a connection with no successor and no replica drain
//! asks at all.

use std::{sync::Arc, time::Duration};

use af_core::{
    clock::{self, FixedClock},
    id::Uuid7,
};
use afd_bus::wake::{Generation, Wake};
use afd_fleet::run::AssignedRun;
use jiff::Timestamp;
use tokio::sync::watch;

use super::depart;
use crate::connection::{ConnectionHandle, cache::RunCache};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

/// The wall-clock instant every connection here ends at.
const NOW: Timestamp = Timestamp::constant(1_790_000_000, 0);

/// The finest step a deadline takes in the column and on the wire.
const ONE_MILLISECOND: Duration = Duration::from_millis(1);

fn id(index: u32) -> af_core::error::Result<Uuid7> {
    Uuid7::parse(&format!("019f0000-0000-7000-8000-{index:012x}"))
}

/// A gateway whose wall clock stands at [`NOW`], requesting passes on `wake`,
/// over a database that never answers: the silence visit's read fails and is
/// observed, so only the lapse can touch `wake`.
fn gateway(wake: &Wake) -> Result<crate::Gateway, Box<dyn std::error::Error + Send + Sync>> {
    crate::fixture::clocked(
        afd_db::test_util::unreachable_db(),
        afd_bus::test_util::unreachable(),
        wake.clone(),
        Arc::new(FixedClock::at(NOW)),
    )
}

/// Holds run `index`, lapsing `after` [`NOW`].
fn hold(cache: &RunCache, index: u32, after: Duration) -> crate::Result<Uuid7> {
    let run = id(index)?;
    cache.insert(
        AssignedRun {
            run_id: run.clone(),
            expires_at: clock::after(NOW, after),
            target: None,
            reported: None,
        },
        NOW,
    )?;
    Ok(run)
}

/// Whether a pass was requested since the last look; marks it seen.
fn requested(
    generations: &mut watch::Receiver<Generation>,
) -> Result<bool, watch::error::RecvError> {
    let changed = generations.has_changed()?;
    generations.borrow_and_update();
    Ok(changed)
}

/// Moves the paused clock and lets the departed connection's task run.
async fn advance(span: Duration) {
    tokio::time::advance(span).await;
    tokio::task::yield_now().await;
}

#[tokio::test(start_paused = true)]
async fn a_connection_that_ends_holding_live_runs_requests_one_pass_at_the_last_lapse() -> TestResult
{
    let wake = Wake::default();
    let gateway = gateway(&wake)?;
    let handle = ConnectionHandle::fixture(id(1)?, id(2)?);
    let cache = RunCache::default();
    hold(&cache, 3, Duration::from_secs(20))?;
    hold(&cache, 4, Duration::from_secs(30))?;
    let cancelled = hold(&cache, 5, Duration::from_secs(90))?;
    cache.cancel(&cancelled);
    let mut generations = wake.generations();
    generations.borrow_and_update();

    depart(&gateway, &handle, &cache);
    advance(Duration::from_secs(20)).await;
    assert!(
        !requested(&mut generations)?,
        "the earlier lapse leaves the later run held"
    );
    advance(Duration::from_millis(9_999)).await;
    assert!(!requested(&mut generations)?, "no pass before the lapse");
    advance(ONE_MILLISECOND).await;
    assert!(requested(&mut generations)?, "one pass at the lapse");
    assert!(wake.recovery().is_some(), "the pass is a full recheck");
    advance(Duration::from_secs(600)).await;
    assert!(
        !requested(&mut generations)?,
        "the cancelled run's later deadline asks for nothing"
    );
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_connection_that_held_nothing_live_requests_no_pass() -> TestResult {
    let wake = Wake::default();
    let gateway = gateway(&wake)?;
    let handle = ConnectionHandle::fixture(id(1)?, id(2)?);
    let cache = RunCache::default();
    let cancelled = hold(&cache, 3, Duration::from_secs(30))?;
    cache.cancel(&cancelled);
    let mut generations = wake.generations();
    generations.borrow_and_update();

    depart(&gateway, &handle, &cache);
    advance(Duration::from_secs(600)).await;
    assert!(!requested(&mut generations)?);
    assert!(wake.recovery().is_none());
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_connection_whose_runs_already_lapsed_requests_no_pass() -> TestResult {
    let wake = Wake::default();
    let gateway = gateway(&wake)?;
    let handle = ConnectionHandle::fixture(id(1)?, id(2)?);
    let cache = RunCache::default();
    hold(&cache, 3, Duration::ZERO)?;
    let mut generations = wake.generations();
    generations.borrow_and_update();

    // A run whose deadline is the departure instant is not live: the
    // assignment that lapsed it was already free to run.
    depart(&gateway, &handle, &cache);
    advance(Duration::from_secs(600)).await;
    assert!(!requested(&mut generations)?);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_superseded_connection_schedules_nothing() -> TestResult {
    let wake = Wake::default();
    let gateway = gateway(&wake)?;
    let handle = ConnectionHandle::fixture(id(1)?, id(2)?);
    let cache = RunCache::default();
    hold(&cache, 3, Duration::from_secs(30))?;
    let mut generations = wake.generations();
    generations.borrow_and_update();

    handle.supersede();
    depart(&gateway, &handle, &cache);
    assert_eq!(gateway.in_flight(), 0, "a successor owns the runner");
    advance(Duration::from_secs(600)).await;
    assert!(!requested(&mut generations)?);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_connection_closed_by_a_drain_schedules_nothing() -> TestResult {
    let wake = Wake::default();
    let gateway = gateway(&wake)?;
    let handle = ConnectionHandle::fixture(id(1)?, id(2)?);
    let cache = RunCache::default();
    hold(&cache, 3, Duration::from_secs(30))?;
    let mut generations = wake.generations();
    generations.borrow_and_update();

    handle.drain.cancel();
    depart(&gateway, &handle, &cache);
    assert_eq!(gateway.in_flight(), 0, "the replica is stopping");
    advance(Duration::from_secs(600)).await;
    assert!(!requested(&mut generations)?);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_replica_drain_ends_a_waiting_pass_without_waiting_for_it() -> TestResult {
    let wake = Wake::default();
    let gateway = gateway(&wake)?;
    let handle = ConnectionHandle::fixture(id(1)?, id(2)?);
    let cache = RunCache::default();
    hold(&cache, 3, Duration::from_secs(30))?;
    let mut generations = wake.generations();
    generations.borrow_and_update();

    depart(&gateway, &handle, &cache);
    assert_eq!(
        gateway.in_flight(),
        1,
        "the departure waits in the task set"
    );
    gateway
        .drain(tokio::time::Instant::now() + Duration::from_secs(1))
        .await?;
    assert_eq!(gateway.in_flight(), 0);
    advance(Duration::from_secs(600)).await;
    assert!(
        !requested(&mut generations)?,
        "the next boot's read owns the pass"
    );
    Ok(())
}
