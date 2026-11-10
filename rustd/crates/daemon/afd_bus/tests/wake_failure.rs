//! Exact Ring accounting has its own binary because producers are process-wide.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use af_core::{id::Uuid7, timing::READY_WAKE_DEADLINE};
use af_observability::test_util::Capture;
use afd_bus::{Bus, Handle, Result, Subject, Subscription, wake};
use async_trait::async_trait;

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug)]
struct Silent {
    publishes: Arc<AtomicUsize>,
}

#[async_trait]
impl Bus for Silent {
    async fn publish(&self, _subject: &Subject, _payload: &str) -> Result<()> {
        self.publishes.fetch_add(1, Ordering::SeqCst);
        std::future::pending().await
    }

    async fn publish_batch(&self, _subject: &Subject, _payloads: &[String]) -> Result<()> {
        std::future::pending().await
    }

    async fn subscribe(&self, _subject: &Subject) -> Result<Subscription> {
        std::future::pending().await
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

#[tokio::test(start_paused = true)]
async fn a_failed_ring_is_counted_once_after_its_publish_deadline() -> TestResult {
    let capture = Capture::install();
    let family = "agentsfleet_fleet_ready_wake_failures_total";
    let labels = &[("kind", "ring")];
    let before = capture.sum(family, labels);
    let publishes = Arc::new(AtomicUsize::new(0));
    let bus = Handle::new(Silent {
        publishes: Arc::clone(&publishes),
    });
    let fleet = Uuid7::parse("0195b4ba-8d3a-7000-8abc-00000000e001")?;
    let started = tokio::time::Instant::now();
    wake::publish(&bus, fleet).await;
    assert_eq!(started.elapsed(), READY_WAKE_DEADLINE);
    assert_eq!(publishes.load(Ordering::SeqCst), 1);
    assert_eq!(capture.sum(family, labels), before + 1);
    bus.shutdown().await?;
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_silent_runner_command_preserves_its_deadline_cause() -> TestResult {
    let publishes = Arc::new(AtomicUsize::new(0));
    let bus = Handle::new(Silent {
        publishes: Arc::clone(&publishes),
    });
    let runner = Uuid7::parse("0195b4ba-8d3a-7000-8abc-00000000e002")?;
    let started = tokio::time::Instant::now();
    let failure = afd_bus::Command::Recheck
        .publish(&bus, &runner)
        .await
        .err()
        .ok_or("a silent broker unexpectedly completed")?;
    assert_eq!(failure.reason(), "timeout");
    assert_eq!(
        failure.code(),
        af_core::error_code::INTERNAL_OPERATION_FAILED
    );
    assert!(
        std::error::Error::source(&failure)
            .is_some_and(<dyn std::error::Error + 'static>::is::<tokio::time::error::Elapsed>)
    );
    assert_eq!(started.elapsed(), READY_WAKE_DEADLINE);
    assert_eq!(publishes.load(Ordering::SeqCst), 1);
    bus.shutdown().await?;
    Ok(())
}
