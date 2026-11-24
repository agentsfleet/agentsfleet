//! Exact Ring accounting has its own binary because producers are process-wide.

use af_core::id::Uuid7;
use af_observability::test_util::Capture;
use afd_bus::{Result, wake};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

/// A bus that refuses every publish, as a shut one does.
async fn refusing() -> Result<afd_bus::Handle> {
    let (bus, _server) = afd_bus::test_util::detached()?;
    bus.shutdown().await?;
    Ok(bus)
}

#[tokio::test]
async fn a_failed_ring_is_counted_once() -> TestResult {
    let capture = Capture::install();
    let family = "agentsfleet_fleet_ready_wake_failures_total";
    let labels = &[("kind", "ring")];
    let before = capture.sum(family, labels);
    let bus = refusing().await?;
    let fleet = Uuid7::parse("0195b4ba-8d3a-7000-8abc-00000000e001")?;
    wake::publish(&bus, fleet).await;
    assert_eq!(capture.sum(family, labels), before + 1);
    Ok(())
}

#[tokio::test]
async fn a_refused_runner_command_returns_the_bus_failure() -> TestResult {
    let bus = refusing().await?;
    let runner = Uuid7::parse("0195b4ba-8d3a-7000-8abc-00000000e002")?;
    let failure = afd_bus::Command::Recheck
        .publish(&bus, &runner)
        .await
        .err()
        .ok_or("a shut bus unexpectedly carried the command")?;
    assert_eq!(failure.reason(), "closed");
    assert_eq!(
        failure.code(),
        af_core::error_code::INTERNAL_OPERATION_FAILED
    );
    Ok(())
}
