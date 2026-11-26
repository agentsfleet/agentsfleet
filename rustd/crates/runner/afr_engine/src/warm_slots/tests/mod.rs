#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fake it cannot build"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use self::fakes::{Counting, settle};
use super::WarmSlots;
use crate::engine::{Engine, Limits, SandboxSpec, fresh_sandbox_id};
use crate::network::{Allowlist, Network};

mod fakes;

#[tokio::test]
async fn test_warm_slot_single_use() {
    let inner = Arc::new(Counting::default());
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    settle().await;
    let (run_a, run_b) = (fresh_sandbox_id(), fresh_sandbox_id());

    let first = slots
        .prepare(SandboxSpec::granted_by(&slots, &run_a, Limits::DEFAULT).unwrap())
        .await
        .unwrap();
    settle().await;
    let second = slots
        .prepare(SandboxSpec::granted_by(&slots, &run_b, Limits::DEFAULT).unwrap())
        .await
        .unwrap();

    assert_ne!(first.id(), &run_a, "warm: the slot's own identifier");
    assert_ne!(second.id(), &run_b, "warm: the slot's own identifier");
    assert_ne!(
        first.id(),
        second.id(),
        "a replacement, never the first again"
    );
    first.destroy().await.unwrap();
    second.destroy().await.unwrap();
    settle().await;
    assert_eq!(
        inner.started.load(Ordering::SeqCst),
        3,
        "two handed out, one waiting"
    );
    slots.shutdown().await;
    assert_eq!(
        inner.destroyed.load(Ordering::SeqCst),
        3,
        "the waiting slot is retired too"
    );
}

#[tokio::test]
async fn test_a_request_with_other_limits_starts_cold() {
    let inner = Arc::new(Counting::default());
    let capture = af_core::test_util::trace::Capture::install();
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    settle().await;
    let run_c = fresh_sandbox_id();
    let other = Limits {
        pids: 7,
        ..Limits::default()
    };

    let cold = slots
        .prepare(SandboxSpec::granted_by(&slots, &run_c, other).unwrap())
        .await
        .unwrap();

    assert_eq!(cold.id(), &run_c, "cold: built under the asked identifier");
    assert_eq!(
        capture.only("sandbox_start_completed").field("start"),
        Some("cold")
    );
    cold.destroy().await.unwrap();
    slots.shutdown().await;
}

#[tokio::test]
async fn test_zero_slots_makes_every_start_cold() {
    let inner = Arc::new(Counting::default());
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 0, Limits::default());

    let run_d = fresh_sandbox_id();
    let cold = slots
        .prepare(SandboxSpec::granted_by(&slots, &run_d, Limits::DEFAULT).unwrap())
        .await
        .unwrap();

    assert_eq!(cold.id(), &run_d);
    cold.destroy().await.unwrap();
    slots.shutdown().await;
}

/// A start that keeps failing is retried with backoff for as long as the
/// keeper runs, each failure logged with its code; shutdown cuts the wait short.
#[tokio::test(start_paused = true)]
async fn test_a_slot_that_keeps_failing_is_retried_until_shutdown() {
    let inner = Arc::new(Counting {
        refusals: AtomicU64::new(u64::MAX),
        ..Counting::default()
    });
    let capture = af_core::test_util::trace::Capture::install();
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    tokio::time::sleep(Duration::from_secs(5)).await;

    let run_e = fresh_sandbox_id();
    let refused = slots
        .prepare(SandboxSpec::granted_by(&slots, &run_e, Limits::DEFAULT).unwrap())
        .await;
    slots.shutdown().await;

    assert!(refused.is_err(), "the cold start is refused the same way");
    let failures: Vec<_> = capture
        .events()
        .into_iter()
        .filter(|event| event.field("event") == Some("sandbox_warm_slot_failed"))
        .collect();
    assert!(failures.len() >= 3, "retried: {failures:?}");
    assert!(
        failures
            .iter()
            .all(|event| event.field("error_code").is_some())
    );
}

/// A host that refused a few starts still ends up with its slot.
#[tokio::test(start_paused = true)]
async fn test_a_slot_that_failed_a_few_times_is_started_after_all() {
    let inner = Arc::new(Counting {
        refusals: AtomicU64::new(2),
        ..Counting::default()
    });
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    tokio::time::sleep(Duration::from_secs(5)).await;

    let run_g = fresh_sandbox_id();
    let warm = slots
        .prepare(SandboxSpec::granted_by(&slots, &run_g, Limits::DEFAULT).unwrap())
        .await
        .unwrap();

    assert_ne!(warm.id(), &run_g, "warm: the slot's own identifier");
    warm.destroy().await.unwrap();
    slots.shutdown().await;
}

/// A slot whose sandbox died while it waited is retired, never handed out.
#[tokio::test]
async fn test_a_slot_that_died_is_never_handed_to_a_run() {
    let inner = Arc::new(Counting {
        dead: true,
        ..Counting::default()
    });
    let capture = af_core::test_util::trace::Capture::install();
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    settle().await;

    let run_h = fresh_sandbox_id();
    let cold = slots
        .prepare(SandboxSpec::granted_by(&slots, &run_h, Limits::DEFAULT).unwrap())
        .await
        .unwrap();
    settle().await;

    assert_eq!(cold.id(), &run_h, "started cold instead");
    assert!(
        capture
            .only("sandbox_warm_slot_died")
            .field("error_code")
            .is_some()
    );
    assert!(
        inner.destroyed.load(Ordering::SeqCst) >= 1,
        "the dead slot was retired"
    );
    assert_eq!(
        inner.started.load(Ordering::SeqCst),
        3,
        "the dead slot, its replacement, and the cold start"
    );
    cold.destroy().await.unwrap();
    slots.shutdown().await;
}

/// A slot still starting when the keeper shuts down is waited for and torn
/// down, never abandoned with its cgroup and disk in place.
#[tokio::test(start_paused = true)]
async fn test_shutdown_waits_for_a_start_still_in_flight() {
    let inner = Arc::new(Counting {
        delay: Duration::from_secs(1),
        ..Counting::default()
    });
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    settle().await;
    assert_eq!(
        inner.started.load(Ordering::SeqCst),
        1,
        "the slot is mid-start"
    );

    slots.shutdown().await;

    assert_eq!(inner.destroyed.load(Ordering::SeqCst), 1);
}

/// A run that stops waiting after its claim is answered never strands the
/// slot it was handed: the keeper takes it back and tears it down.
#[tokio::test]
async fn test_a_slot_whose_run_stopped_waiting_is_retired() {
    use futures_util::FutureExt as _;
    let inner = Arc::new(Counting::default());
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    settle().await;

    let run_f = fresh_sandbox_id();
    let abandoned = slots
        .prepare(SandboxSpec::granted_by(&slots, &run_f, Limits::DEFAULT).unwrap())
        .now_or_never();
    settle().await;

    assert!(
        abandoned.is_none(),
        "the claim was still waiting when dropped"
    );
    assert_eq!(inner.destroyed.load(Ordering::SeqCst), 1);
    slots.shutdown().await;
}

/// A waiting slot whose teardown is refused at shutdown is logged with its
/// code and sandbox identifier, and the keeper still stops.
#[tokio::test]
async fn test_a_slot_whose_teardown_is_refused_is_logged_as_left() {
    let inner = Arc::new(Counting {
        leaks: true,
        ..Counting::default()
    });
    let capture = af_core::test_util::trace::Capture::install();
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    settle().await;

    slots.shutdown().await;

    let left = capture.only("sandbox_warm_slot_left");
    assert!(left.field("error_code").is_some());
    assert!(left.field("sandbox_id").is_some());
    assert_eq!(inner.destroyed.load(Ordering::SeqCst), 1);
}

/// A keeper that dies is reported by shutdown rather than ignored.
#[tokio::test]
async fn test_a_keeper_that_dies_is_reported_at_shutdown() {
    let inner = Arc::new(Counting {
        panics: true,
        ..Counting::default()
    });
    let capture = af_core::test_util::trace::Capture::install();
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    settle().await;

    slots.shutdown().await;

    assert!(
        capture
            .only("sandbox_warm_keeper_failed")
            .field("reason")
            .is_some_and(|reason| reason.contains("panicked"))
    );
}

/// A slot is built isolated, so a run that asks for the host's network or
/// an allowlist is never handed one: its namespace was chosen when the slot
/// started, under whatever the runner was assigned then.
#[tokio::test]
async fn test_warm_slot_serves_only_its_own_network() {
    let inner = Arc::new(Counting::default());
    let capture = af_core::test_util::trace::Capture::install();
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    settle().await;
    let allowlist = Allowlist::new(Vec::new()).unwrap();

    for network in [Network::Host, Network::Allowed(&allowlist)] {
        let asked = fresh_sandbox_id();
        let spec = SandboxSpec::granted_by(&slots, &asked, Limits::DEFAULT)
            .unwrap()
            .with_egress(crate::EgressPlan {
                generation: crate::Generation(1),
                network,
            });
        let cold = slots.prepare(spec).await.unwrap();
        assert_eq!(cold.id(), &asked);
        cold.destroy().await.unwrap();
    }
    let run_i = fresh_sandbox_id();
    let warm = slots
        .prepare(SandboxSpec::granted_by(&slots, &run_i, Limits::DEFAULT).unwrap())
        .await
        .unwrap();

    assert_ne!(warm.id(), &run_i, "warm: the slot's own identifier");
    let starts: Vec<_> = capture
        .events()
        .iter()
        .filter(|line| line.field("event") == Some("sandbox_start_completed"))
        .map(|line| line.field("start").map(str::to_owned))
        .collect();
    assert_eq!(
        starts,
        ["cold", "cold", "warm"].map(|start| Some(start.to_owned()))
    );
    warm.destroy().await.unwrap();
    slots.shutdown().await;
}
