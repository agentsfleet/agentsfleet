//! Every writer that meets the slot and its head admission, released
//! together, takes them in one order and never deadlocks.

use std::sync::Arc;
use std::time::Duration;

use afd_approval::Decision;
use afd_fleet::lease::admit::Refusal;

use super::admit_at;
use crate::seed::seeded_parts;
use crate::support::Fixtures;

/// A refusal label the fixtures end an event with.
const ENDED: Refusal = Refusal::labelled("fixture_ended");

/// Claims, endings, parks, gate resolutions and admissions over
/// ten fleets, released together, never deadlock. Every fleet carries a
/// standing card, so a park parks and a resolution has a head to unpark.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_claim_report_park_and_admit_never_deadlock() {
    const FLEETS: usize = 10;
    const EACH: usize = 50;
    const KINDS: usize = 5;
    let fixtures = Arc::new(Fixtures::create_isolated_with_queue().await);
    let mut seeded = Vec::new();
    for _ in 0..FLEETS {
        let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
        for offset in 0..3 {
            admit_at(&fixtures, &fleet, &workspace, offset).await;
        }
        let card = super::approvals::pending_card(
            &fixtures,
            &fleet,
            &workspace,
            None,
            Duration::from_secs(3600),
        )
        .await;
        seeded.push((fleet, workspace, runner, card));
    }
    let seeded = Arc::new(seeded);
    let barrier = Arc::new(tokio::sync::Barrier::new(EACH * KINDS));
    let mut tasks = Vec::new();
    for index in 0..EACH {
        for kind in 0..KINDS {
            let (fixtures, seeded, barrier) = (
                Arc::clone(&fixtures),
                Arc::clone(&seeded),
                Arc::clone(&barrier),
            );
            tasks.push(tokio::spawn(async move {
                let (fleet, workspace, runner, card) = seeded
                    .get(index % FLEETS)
                    .expect("a fleet per slot")
                    .clone();
                barrier.wait().await;
                contend(&fixtures, kind, &fleet, &workspace, &runner, &card, index).await
            }));
        }
    }
    let mut failures = Vec::new();
    for task in tasks {
        if let Err(failure) = task.await.expect("the task joins") {
            failures.push(failure);
        }
    }

    let deadlocks: Vec<_> = failures
        .iter()
        .filter(|failure| failure.contains("40P01") || failure.contains("deadlock"))
        .collect();
    assert!(deadlocks.is_empty(), "{deadlocks:?}");
    assert!(failures.is_empty(), "no writer faulted: {failures:?}");
    Arc::into_inner(fixtures)
        .expect("every task has finished")
        .cleanup()
        .await;
}

/// One contender: a claim, an ending of what it claimed, a park of what it
/// claimed, a gate resolution, or an admission.
async fn contend(
    fixtures: &Fixtures,
    kind: usize,
    fleet: &str,
    workspace: &str,
    runner: &afd_core::id::Uuid7,
    card: &str,
    index: usize,
) -> Result<(), String> {
    let leases = fixtures.leases();
    match kind {
        0 => leases
            .select(runner, &[])
            .await
            .map(drop)
            .map_err(|e| e.to_string()),
        1 => match leases
            .select(runner, &[])
            .await
            .map_err(|e| e.to_string())?
        {
            Some(claimed) => leases
                .block(&claimed, ENDED)
                .await
                .map(drop)
                .map_err(|e| e.to_string()),
            None => Ok(()),
        },
        2 => match leases
            .select(runner, &[])
            .await
            .map_err(|e| e.to_string())?
        {
            Some(claimed) => leases
                .park(&claimed.fleet_id, &claimed.event_id, claimed.fence)
                .await
                .map_err(|e| e.to_string()),
            None => Ok(()),
        },
        3 => super::approvals::inbox(fixtures)
            .resolve(card, Decision::Approved, "fixture:human", "", Some(fleet))
            .await
            .map(drop)
            .map_err(|e| e.to_string()),
        _ => {
            crate::queue::enqueue(
                &fixtures.database,
                fleet,
                workspace,
                crate::seed::ACTOR,
                crate::seed::EVENT_TYPE,
                crate::seed::REQUEST_JSON,
                afd_core::clock::after(
                    fixtures.enrolled_at(),
                    Duration::from_secs(10 + index as u64),
                ),
            )
            .await;
            Ok(())
        }
    }
}
