//! A recovery hint fills real connection credits without another five-second probe.

use super::*;
use crate::connection::AssignMode;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::{sync::Notify, task::JoinSet};

#[tokio::test(start_paused = true)]
async fn one_recovery_wake_drains_one_ten_one_hundred_and_five_hundred_fleets() -> TestResult {
    for fleets in [1, 10, 100, 500] {
        drain_burst(fleets).await?;
    }
    Ok(())
}

async fn drain_burst(fleets: usize) -> TestResult {
    let registry = Arc::new(Registry::default());
    let wake = Wake::default();
    let stop = CancellationToken::new();
    let _cancel = stop.clone().drop_guard();
    let remaining = Arc::new(AtomicUsize::new(fleets));
    let completed = Arc::new(Notify::new());
    let mut registrations = Vec::new();
    let mut hosts = JoinSet::new();
    let maximum = af_wire::protocol::RUNS_MAX;
    for number in 1..=fleets.div_ceil(maximum) {
        let connection = ConnectionFixture::new(number)?;
        connection.handle.snapshot.lock().credits = maximum;
        registrations.push(connection.register(&registry)?);
        hosts.spawn(serve_host(
            connection,
            Arc::clone(&remaining),
            Arc::clone(&completed),
        ));
    }
    let scheduler = schedule(&registry, &wake, &stop);
    tokio::task::yield_now().await;
    let started = tokio::time::Instant::now();
    wake.recheck();
    let result = tokio::time::timeout(
        Duration::from_secs(1),
        claims_drained(&completed, &remaining),
    )
    .await;
    stop.cancel();
    scheduler.await??;
    hosts.shutdown().await;
    assert!(
        result.is_ok(),
        "one wake left {} of {fleets} fleets waiting",
        remaining.load(Ordering::SeqCst)
    );
    assert!(started.elapsed() < af_core::timing::ASSIGN_FALLBACK_TICK);
    drop(registrations);
    Ok(())
}

fn schedule(
    registry: &Arc<Registry>,
    wake: &Wake,
    stop: &CancellationToken,
) -> tokio::task::JoinHandle<crate::Result<()>> {
    tokio::spawn({
        let registry = Arc::clone(registry);
        let wake = wake.clone();
        let stop = stop.clone();
        async move { notifications(&registry, &wake, 8, stop, async |_| Ok(super::everyone())).await }
    })
}

async fn claims_drained(completed: &Notify, remaining: &AtomicUsize) {
    loop {
        let observed = completed.notified();
        if remaining.load(Ordering::SeqCst) == 0 {
            return;
        }
        observed.await;
    }
}

async fn serve_host(
    mut connection: ConnectionFixture,
    remaining: Arc<AtomicUsize>,
    completed: Arc<Notify>,
) -> TestResult {
    while let Some(answer) = connection.requests.recv().await {
        let available = connection.handle.snapshot.lock().credits > 0;
        let issued = available
            && remaining
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                    left.checked_sub(1)
                })
                .is_ok();
        if issued {
            connection.handle.snapshot.lock().credits -= 1;
        }
        let _finished = answer.reply.send(if issued {
            AssignOutcome::Issued
        } else {
            AssignOutcome::Empty
        });
        completed.notify_one();
    }
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_slow_host_does_not_hold_back_a_different_eligible_host() -> TestResult {
    let registry = Arc::new(Registry::default());
    let mut slow = ConnectionFixture::new(1)?;
    tokio::time::advance(Duration::from_millis(1)).await;
    let mut healthy = ConnectionFixture::new(2)?;
    let _slow = slow.register(&registry)?;
    let _healthy = healthy.register(&registry)?;
    let wake = Wake::default();
    let stop = CancellationToken::new();
    let _cancel = stop.clone().drop_guard();
    let scheduler = tokio::spawn({
        let registry = Arc::clone(&registry);
        let wake = wake.clone();
        let stop = stop.clone();
        async move { notifications(&registry, &wake, 8, stop, async |_| Ok(super::everyone())).await }
    });
    tokio::task::yield_now().await;
    wake.recheck();
    let blocked = slow.request().await?;
    let eligible = tokio::time::timeout(Duration::from_secs(1), healthy.requests.recv()).await;
    stop.cancel();
    scheduler.await??;
    drop(blocked);
    assert!(
        matches!(eligible, Ok(Some(_))),
        "the idle healthy host waited for the slow host"
    );
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn an_ordinary_ring_can_reach_a_free_host_while_recovery_is_waiting() -> TestResult {
    let registry = Arc::new(Registry::default());
    let mut slow = ConnectionFixture::new(1)?;
    let mut healthy = ConnectionFixture::new(2)?;
    healthy
        .handle
        .snapshot
        .lock()
        .held
        .push(healthy.handle.runner.clone());
    let _slow = slow.register(&registry)?;
    let _healthy = healthy.register(&registry)?;
    let wake = Wake::default();
    let stop = CancellationToken::new();
    let _cancel = stop.clone().drop_guard();
    let scheduler = tokio::spawn({
        let registry = Arc::clone(&registry);
        let wake = wake.clone();
        let stop = stop.clone();
        async move { notifications(&registry, &wake, 8, stop, async |_| Ok(super::everyone())).await }
    });
    tokio::task::yield_now().await;
    wake.recheck();
    let blocked = slow.request().await?;
    assert_eq!(blocked.mode, AssignMode::Recovery);
    let recovered = healthy.request().await?;
    assert_eq!(recovered.mode, AssignMode::Recovery);
    recovered
        .reply
        .send(AssignOutcome::Empty)
        .map_err(|_closed| "recovery recipient closed")?;

    wake.ring(healthy.handle.runner.clone());
    let ordinary = healthy.request().await?;
    assert_eq!(ordinary.mode, AssignMode::Ordinary);
    ordinary
        .reply
        .send(AssignOutcome::Issued)
        .map_err(|_closed| "ordinary recipient closed")?;
    stop.cancel();
    scheduler.await??;
    drop(blocked);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_new_recovery_wake_survives_the_previous_pass_finishing() -> TestResult {
    let registry = Arc::new(Registry::default());
    let mut host = ConnectionFixture::new(1)?;
    let _registration = host.register(&registry)?;
    let wake = Wake::default();
    let stop = CancellationToken::new();
    let _cancel = stop.clone().drop_guard();
    let scheduler = tokio::spawn({
        let registry = Arc::clone(&registry);
        let wake = wake.clone();
        let stop = stop.clone();
        async move { notifications(&registry, &wake, 8, stop, async |_| Ok(super::everyone())).await }
    });
    tokio::task::yield_now().await;
    wake.recheck();
    let first = host.request().await?;
    let earlier = wake.recovery();
    wake.recheck();
    assert_ne!(earlier, wake.recovery());
    first
        .reply
        .send(AssignOutcome::Empty)
        .map_err(|_closed| "first recovery recipient closed")?;
    let second = host.request().await?;
    assert_eq!(second.mode, AssignMode::Recovery);
    second
        .reply
        .send(AssignOutcome::Empty)
        .map_err(|_closed| "second recovery recipient closed")?;
    stop.cancel();
    scheduler.await??;
    Ok(())
}
