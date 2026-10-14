#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fake it cannot build"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use self::fakes::{Counting, name_of, settle};
use super::WarmSlots;
use crate::engine::{Engine, Limits, SandboxRequest};
use crate::network::{Allowlist, Network};

mod fakes;

#[tokio::test]
async fn test_warm_slot_single_use() {
    let inner = Arc::new(Counting::default());
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    settle().await;
    let request = SandboxRequest::new("lease-a", Limits::default());

    let first = slots.prepare(request).await.unwrap();
    settle().await;
    let second = slots
        .prepare(SandboxRequest {
            lease_id: "lease-b",
            ..request
        })
        .await
        .unwrap();

    let (first_name, second_name) = (name_of(first.as_ref()), name_of(second.as_ref()));
    assert!(first_name.contains("\"warm-"), "{first_name}");
    assert!(second_name.contains("\"warm-"), "{second_name}");
    assert_ne!(
        first_name, second_name,
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
    let capture = afd_core::test_util::trace::Capture::install();
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    settle().await;

    let cold = slots
        .prepare(SandboxRequest::new(
            "lease-c",
            Limits {
                pids: 7,
                ..Limits::default()
            },
        ))
        .await
        .unwrap();

    assert!(name_of(cold.as_ref()).contains("lease-c"));
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

    let cold = slots
        .prepare(SandboxRequest::new("lease-d", Limits::default()))
        .await
        .unwrap();

    assert!(name_of(cold.as_ref()).contains("lease-d"));
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
    let capture = afd_core::test_util::trace::Capture::install();
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    tokio::time::sleep(Duration::from_secs(5)).await;

    let refused = slots
        .prepare(SandboxRequest::new("lease-e", Limits::default()))
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

    let warm = slots
        .prepare(SandboxRequest::new("lease-g", Limits::default()))
        .await
        .unwrap();

    assert!(name_of(warm.as_ref()).contains("\"warm-"), "{warm:?}");
    warm.destroy().await.unwrap();
    slots.shutdown().await;
}

/// A slot whose sandbox died while it waited is retired, never handed out.
#[tokio::test]
async fn test_a_slot_that_died_is_never_handed_to_a_lease() {
    let inner = Arc::new(Counting {
        dead: true,
        ..Counting::default()
    });
    let capture = afd_core::test_util::trace::Capture::install();
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    settle().await;

    let cold = slots
        .prepare(SandboxRequest::new("lease-h", Limits::default()))
        .await
        .unwrap();
    settle().await;

    assert!(
        name_of(cold.as_ref()).contains("lease-h"),
        "started cold instead"
    );
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

/// A lease that stops waiting after its claim is answered never strands the
/// slot it was handed: the keeper takes it back and tears it down.
#[tokio::test]
async fn test_a_slot_whose_lease_stopped_waiting_is_retired() {
    use futures_util::FutureExt as _;
    let inner = Arc::new(Counting::default());
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    settle().await;

    let abandoned = slots
        .prepare(SandboxRequest::new("lease-f", Limits::default()))
        .now_or_never();
    settle().await;

    assert!(
        abandoned.is_none(),
        "the claim was still waiting when dropped"
    );
    assert_eq!(inner.destroyed.load(Ordering::SeqCst), 1);
    slots.shutdown().await;
}

/// A keeper that dies is reported by shutdown rather than ignored.
#[tokio::test]
async fn test_a_keeper_that_dies_is_reported_at_shutdown() {
    let inner = Arc::new(Counting {
        panics: true,
        ..Counting::default()
    });
    let capture = afd_core::test_util::trace::Capture::install();
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

/// A slot is built isolated, so a lease that asks for the host's network or
/// an allowlist is never handed one: its namespace was chosen when the slot
/// started, under whatever the runner was assigned then.
#[tokio::test]
async fn test_warm_slot_serves_only_its_own_network() {
    let inner = Arc::new(Counting::default());
    let capture = afd_core::test_util::trace::Capture::install();
    let slots = WarmSlots::start(Arc::clone(&inner) as Arc<dyn Engine>, 1, Limits::default());
    settle().await;
    let allowlist = Allowlist::new(Vec::new()).unwrap();

    for (lease_id, network) in [
        ("lease-h", Network::Host),
        ("lease-a", Network::Allowed(&allowlist)),
    ] {
        let cold = slots
            .prepare(SandboxRequest::new(lease_id, Limits::default()).with_network(network))
            .await
            .unwrap();
        assert!(name_of(cold.as_ref()).contains(lease_id));
        cold.destroy().await.unwrap();
    }
    let warm = slots
        .prepare(SandboxRequest::new("lease-i", Limits::default()))
        .await
        .unwrap();

    assert!(name_of(warm.as_ref()).contains("\"warm-"));
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
