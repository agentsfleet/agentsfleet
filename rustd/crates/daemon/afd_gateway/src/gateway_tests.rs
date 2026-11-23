//! A replica's gateway keeps its handles out of logs and closes by one deadline.

use std::time::Duration;

use af_core::{error_code, id::Uuid7};
use tokio::time::Instant;

use crate::{connection::ConnectionHandle, error::ErrorKind, fixture};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

fn offline() -> Result<crate::Gateway, Box<dyn std::error::Error + Send + Sync>> {
    fixture::gateway(
        afd_db::test_util::unreachable_db(),
        afd_bus::test_util::unreachable(),
        afd_bus::wake::Wake::default(),
    )
}

#[tokio::test]
async fn the_gateway_and_its_handles_print_no_field() -> TestResult {
    let gateway = offline()?;
    assert_eq!(format!("{gateway:?}"), "Gateway { .. }");
    assert_eq!(
        format!("{:?}", gateway.inner.dependencies),
        "GatewayDependencies { .. }"
    );
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn drain_cancels_every_connection_still_serving_at_its_deadline() -> TestResult {
    let gateway = offline()?;
    let runner = Uuid7::parse("019f0000-0000-7000-8000-000000000001")?;
    let serving = ConnectionHandle::fixture(runner.clone(), runner);
    let _registration = gateway.inner.registry.register_fixture(&serving)?;
    let held = gateway.inner.tasks.spawn(std::future::pending::<()>());
    assert_eq!(gateway.in_flight(), 1);
    let refused = gateway
        .drain(Instant::now() + Duration::from_secs(1))
        .await
        .err()
        .ok_or("a connection that never finished let drain succeed")?;
    assert!(
        matches!(
            refused.kind(),
            ErrorKind::Timeout {
                operation: crate::Operation::Drain,
                ..
            }
        ),
        "{refused:?}"
    );
    assert_eq!(refused.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert!(
        serving.drain.is_cancelled(),
        "drain asked it to finish first"
    );
    assert!(serving.stop.is_cancelled(), "the deadline stopped it");
    assert!(gateway.inner.accounting_stop.is_cancelled());
    held.abort();
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn drain_with_nothing_serving_closes_at_once() -> TestResult {
    let gateway = offline()?;
    gateway.drain(Instant::now()).await?;
    assert_eq!(gateway.in_flight(), 0);
    assert!(!gateway.inner.accounting_stop.is_cancelled());
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn the_scheduler_keeps_running_without_a_broker_until_it_is_stopped() -> TestResult {
    let gateway = offline()?;
    let stop = tokio_util::sync::CancellationToken::new();
    let scheduler = tokio::spawn({
        let gateway = gateway.clone();
        let stop = stop.clone();
        async move { gateway.run(stop).await }
    });
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert!(
        !scheduler.is_finished(),
        "a lost broker ended the scheduler"
    );
    stop.cancel();
    scheduler.await??;
    Ok(())
}
