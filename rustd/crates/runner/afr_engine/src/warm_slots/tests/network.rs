//! Slots are built for the kind of network the daemon assigned, and a claim
//! holds one to its run's own plan.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::fakes::{Counting, Plans, isolated, settle};
use crate::engine::{Engine, Limits, SandboxSpec, fresh_sandbox_id};
use crate::network::{Allowlist, Network, NetworkKind};
use crate::warm_slots::WarmSlots;
use crate::{EgressPlan, Generation};

/// A run's spec under `slots`, asking for `network` under plan `generation`.
fn asking<'a>(
    slots: &WarmSlots,
    id: &'a af_core::id::Uuid7,
    network: Network<'a>,
    generation: u64,
) -> SandboxSpec<'a> {
    SandboxSpec::granted_by(slots, id, Limits::DEFAULT)
        .unwrap()
        .with_egress(EgressPlan {
            generation: Generation(generation),
            network,
        })
}

/// What each run start was labelled, in order.
fn starts(capture: &af_core::test_util::trace::Capture) -> Vec<String> {
    capture
        .events()
        .iter()
        .filter(|line| line.field("event") == Some("sandbox_start_completed"))
        .filter_map(|line| line.field("start").map(str::to_owned))
        .collect()
}

/// A slot built for an allowlist, reaching nothing, serves an allowlisted
/// run once its run's plan is in force; a run of another kind starts cold.
#[tokio::test]
async fn test_an_allowlisted_slot_serves_its_run_under_the_runs_plan() {
    let inner = Arc::new(Counting::default());
    let capture = af_core::test_util::trace::Capture::install();
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    slots.network_assigned(NetworkKind::Allowed);
    settle().await;
    let allowlist = Allowlist::new(vec![("example.test".into(), [192, 0, 2, 7].into())]).unwrap();

    for network in [Network::Host, Network::Isolated] {
        let asked = fresh_sandbox_id();
        let cold = slots
            .prepare(asking(&slots, &asked, network, 1))
            .await
            .unwrap();
        assert_eq!(cold.id(), &asked, "cold: another kind of network");
        cold.destroy().await.unwrap();
    }
    let run = fresh_sandbox_id();
    let warm = slots
        .prepare(asking(&slots, &run, Network::Allowed(&allowlist), 9))
        .await
        .unwrap();

    assert_ne!(warm.id(), &run, "warm: the slot's own identifier");
    assert_eq!(starts(&capture), ["cold", "cold", "warm"]);
    assert_eq!(
        inner.built.lock().unwrap().first(),
        Some(&NetworkKind::Allowed),
        "the slot was built for the assigned kind"
    );
    assert_eq!(
        *inner.plans.held.lock().unwrap(),
        [Generation(9)],
        "the run's plan"
    );
    warm.destroy().await.unwrap();
    slots.shutdown().await;
}

/// A slot that refuses its run's plan is destroyed, and the run starts cold.
#[tokio::test]
async fn test_a_slot_refusing_the_runs_plan_is_destroyed_and_the_run_starts_cold() {
    let inner = Arc::new(Counting {
        plans: Arc::new(Plans {
            refused: true,
            ..Plans::default()
        }),
        ..Counting::default()
    });
    let capture = af_core::test_util::trace::Capture::install();
    let slots = isolated(&inner, 1);
    settle().await;

    let run = fresh_sandbox_id();
    let cold = slots
        .prepare(asking(&slots, &run, Network::Isolated, 3))
        .await
        .unwrap();

    assert_eq!(cold.id(), &run, "cold: built under the asked identifier");
    assert_eq!(starts(&capture), ["cold"]);
    let refused = capture.only("sandbox_warm_slot_unplanned");
    assert!(refused.field("error_code").is_some());
    assert!(refused.field("sandbox_id").is_some());
    assert_eq!(
        inner.destroyed.load(Ordering::SeqCst),
        1,
        "the slot is gone"
    );
    cold.destroy().await.unwrap();
    slots.shutdown().await;
}

/// No slot is built before a kind is assigned; a new kind retires the ready
/// slots, and the next claim of that kind is warm.
#[tokio::test]
async fn test_a_new_kind_retires_the_ready_slots_and_builds_for_itself() {
    let inner = Arc::new(Counting::default());
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    settle().await;
    assert_eq!(inner.started.load(Ordering::SeqCst), 0, "no kind, no slot");

    slots.network_assigned(NetworkKind::Isolated);
    settle().await;
    slots.network_assigned(NetworkKind::Host);
    settle().await;
    assert_eq!(
        inner.destroyed.load(Ordering::SeqCst),
        1,
        "the isolated slot"
    );
    let run = fresh_sandbox_id();
    let warm = slots
        .prepare(asking(&slots, &run, Network::Host, 1))
        .await
        .unwrap();

    assert_ne!(warm.id(), &run, "warm: the slot's own identifier");
    assert_eq!(
        *inner.built.lock().unwrap(),
        [NetworkKind::Isolated, NetworkKind::Host, NetworkKind::Host],
        "the first slot, the one replacing it, and the claimed one's replacement"
    );
    warm.destroy().await.unwrap();
    slots.shutdown().await;
}
