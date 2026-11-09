//! Pushed offers use available workers without polling, including during another report retry.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use af_core::id::Uuid7;
use af_stream::{Budget, MemoryLimits};
use af_wire::{
    lease::TaggedLease,
    protocol::{Down, LEASES_MAX, StreamKind},
};
use tokio::{
    sync::{mpsc, watch},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

use super::serve;
use crate::{
    client::Verb,
    downlink::{Offered, policy::Assignment},
    test_support::{
        Answer, Behaviour, FLEET_ID, FakeAgent, FakeEngine, Rig, bounded, daemon, lease,
    },
};

const FIRST: &str = "01890a5d-ac96-774b-bcce-b302099a8060";
const SECOND: &str = "01890a5d-ac96-774b-bcce-b302099a8061";
const OTHER_FLEET: &str = "01890a5d-ac96-774b-bcce-b302099a8062";

type TestResult<T = ()> = crate::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn assigned(workers: u32) -> Assignment {
    Assignment {
        workers,
        ..Assignment::initial()
    }
}

fn rig() -> Rig {
    Rig::new(
        daemon(|_| None),
        FakeEngine::default(),
        FakeAgent::new(Behaviour::Answer),
    )
}

async fn offer(rig: &Rig, id: &str, fleet: &str) -> TestResult<Offered> {
    let (budget, _) = Budget::duplex(MemoryLimits::default());
    let session = CancellationToken::new();
    let message: Down<'_> = Down::Offer {
        lease: TaggedLease::<serde_json::Value>::Fleet(Box::new(lease(id, fleet))),
    };
    let frame = af_stream::encode(
        &message,
        &budget,
        StreamKind::Work,
        &session,
        Instant::now() + Duration::from_secs(5),
    )
    .await?;
    let registration = rig
        .lessee
        .plane
        .stream
        .running
        .offer(Uuid7::parse(id)?, false)?;
    Ok(Offered {
        frame,
        registration,
        session,
    })
}

async fn until(count: &AtomicUsize, minimum: usize) {
    bounded("worker execution", async {
        while count.load(Ordering::SeqCst) < minimum {
            tokio::task::yield_now().await;
        }
    })
    .await;
}

#[tokio::test]
async fn idle_workers_make_no_requests() -> TestResult {
    let mut rig = rig();
    let (_assignment, watching) = watch::channel(assigned(4));
    let (_offers, offered) = mpsc::channel(LEASES_MAX);
    let pool = tokio::spawn(serve(Arc::clone(&rig.lessee), watching, offered));
    tokio::time::sleep(Duration::from_millis(20)).await;
    rig.shutdown.cancel();
    bounded("idle pool shutdown", pool).await?;
    assert_eq!(rig.runs.load(Ordering::SeqCst), 0);
    assert_eq!(rig.calls().len(), 0);
    Ok(())
}

#[tokio::test]
async fn two_free_workers_start_distinct_fleets() -> TestResult {
    let rig = rig();
    let (_assignment, watching) = watch::channel(assigned(2));
    let (offers, offered) = mpsc::channel(LEASES_MAX);
    offers.send(offer(&rig, FIRST, FLEET_ID).await?).await?;
    offers.send(offer(&rig, SECOND, OTHER_FLEET).await?).await?;
    let pool = tokio::spawn(serve(Arc::clone(&rig.lessee), watching, offered));
    until(&rig.runs, 2).await;
    rig.shutdown.cancel();
    bounded("two-worker pool shutdown", pool).await?;
    assert_eq!(rig.runs.load(Ordering::SeqCst), 2);
    assert_eq!(rig.peak.load(Ordering::SeqCst), 2);
    Ok(())
}

#[tokio::test]
async fn a_free_worker_takes_b_while_a_waits_for_its_report_acknowledgement() -> TestResult {
    let mut rig = Rig::new(
        daemon(|call| {
            if call.verb != Verb::Report {
                return None;
            }
            let Some(body) = call.body.as_ref() else {
                return Some(Answer::Fail(crate::error::config(
                    "a report needs its body",
                )));
            };
            let report: serde_json::Value = match af_core::json::object_from_slice(body) {
                Ok(report) => report,
                Err(source) => {
                    return Some(Answer::Fail(crate::error::malformed(Verb::Report)(source)));
                }
            };
            (report.get("lease_id").and_then(serde_json::Value::as_str) == Some(FIRST))
                .then_some(Answer::Stall)
        }),
        FakeEngine::default(),
        FakeAgent::new(Behaviour::Answer),
    );
    let (_assignment, watching) = watch::channel(assigned(2));
    let (offers, offered) = mpsc::channel(LEASES_MAX);
    offers.send(offer(&rig, FIRST, FLEET_ID).await?).await?;
    let pool = tokio::spawn(serve(Arc::clone(&rig.lessee), watching, offered));
    wait_for_report(&mut rig.calls).await?;
    offers.send(offer(&rig, SECOND, OTHER_FLEET).await?).await?;
    until(&rig.runs, 2).await;
    assert!(
        rig.lessee
            .plane
            .stream
            .running
            .snapshot()
            .contains(&Uuid7::parse(FIRST)?),
        "A's pending report did not stop B from starting on the free worker"
    );
    rig.lessee.halt.stop();
    bounded("reporting pool shutdown", pool).await?;
    Ok(())
}

#[tokio::test]
async fn offers_for_one_fleet_keep_the_existing_turn_order() -> TestResult {
    let rig = rig();
    let (_assignment, watching) = watch::channel(assigned(2));
    let (offers, offered) = mpsc::channel(LEASES_MAX);
    offers.send(offer(&rig, FIRST, FLEET_ID).await?).await?;
    offers.send(offer(&rig, SECOND, FLEET_ID).await?).await?;
    let pool = tokio::spawn(serve(Arc::clone(&rig.lessee), watching, offered));
    until(&rig.runs, 2).await;
    rig.shutdown.cancel();
    bounded("ordered pool shutdown", pool).await?;
    assert_eq!(rig.peak.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn cancellation_before_execution_releases_the_registration_without_running() -> TestResult {
    let rig = rig();
    let (_assignment, watching) = watch::channel(assigned(1));
    let (offers, offered) = mpsc::channel(LEASES_MAX);
    let cancelled = offer(&rig, FIRST, FLEET_ID).await?;
    cancelled.registration.stop.cancel();
    offers.send(cancelled).await?;
    drop(offers);
    let pool = tokio::spawn(serve(Arc::clone(&rig.lessee), watching, offered));
    bounded("cancelled offer removal", async {
        while !rig.lessee.plane.stream.running.snapshot().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await;
    rig.shutdown.cancel();
    bounded("cancelled pool shutdown", pool).await?;
    assert_eq!(rig.runs.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn unread_offers_from_a_disconnected_session_are_not_executed() -> TestResult {
    let rig = rig();
    let (_assignment, watching) = watch::channel(assigned(1));
    let (offers, offered) = mpsc::channel(LEASES_MAX);
    let stale = offer(&rig, FIRST, FLEET_ID).await?;
    stale.session.cancel();
    offers.send(stale).await?;
    drop(offers);
    let pool = tokio::spawn(serve(Arc::clone(&rig.lessee), watching, offered));
    bounded("stale offer removal", async {
        while !rig.lessee.plane.stream.running.snapshot().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await;
    rig.shutdown.cancel();
    bounded("stale pool shutdown", pool).await?;
    assert_eq!(rig.runs.load(Ordering::SeqCst), 0);
    Ok(())
}

async fn wait_for_report(calls: &mut crate::test_support::Calls) -> TestResult {
    bounded("A's report submission", async {
        loop {
            let call = calls
                .recv()
                .await
                .ok_or("the call log closed before A reported")?;
            if call.verb == Verb::Report {
                return Ok::<(), Box<dyn std::error::Error + Send + Sync>>(());
            }
        }
    })
    .await
}
