//! Database-scoped observations prove native progress and the grant's durable host check.
#![cfg(feature = "test-util")]

mod support;

use super::raw::Native;
use crate::wire;
use af_core::timing::STREAM_ANSWER_TIMEOUT;
use af_stream::Transport;
use af_wire::protocol::StreamKind;
use afd_bus::Subject;
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;
use std::time::Duration;
use support::{Counts, Fixture, Held, Tail, healthy, host_locks, offer, text};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_streamed_chat_reads_no_lease_target() -> Result<()> {
    for transport in [Transport::H2, Transport::H3] {
        let mut supervisor = Supervisor::new();
        let fixture = Fixture::open(&mut supervisor).await;
        let measured = measure_cost(&fixture, transport).await;
        fixture.finish(supervisor).await;
        measured?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_grant_rechecks_the_host_inside_its_transaction() -> Result<()> {
    let mut supervisor = Supervisor::new();
    let fixture = Fixture::open(&mut supervisor).await;
    let measured = measure_guard(&fixture).await;
    fixture.finish(supervisor).await;
    measured
}

const ACTIVITY_FRAMES: u64 = 40;

async fn measure_cost(fixture: &Fixture, transport: Transport) -> Result<()> {
    let before = Counts::read(&fixture.owner).await?;
    let mut tail = Tail::open(&fixture.run).await?;
    let mut native = Native::connect(&fixture.run, transport).await;
    let measured = turn(fixture, &mut native, &mut tail).await;
    native.close().await;
    tail.close().await?;
    measured?;
    let after = Counts::read(&fixture.owner).await?;
    assert_eq!(
        after.tokens - before.tokens,
        1,
        "{transport:?}: only the opening reads the runner token"
    );
    assert_eq!(
        after.targets - before.targets,
        0,
        "{transport:?}: all activity uses the native lease cache"
    );
    assert_eq!(
        after.report_reads - before.report_reads,
        0,
        "{transport:?}: the report uses the facts its session kept from the offer"
    );
    Ok(())
}

async fn turn(fixture: &Fixture, native: &mut Native, tail: &mut Tail) -> Result<()> {
    healthy(native, 3).await;
    native.ready().await;
    let held = offer(&fixture.run, native).await?;
    renew(native, &held, 4).await;
    for sequence in 0..ACTIVITY_FRAMES {
        native.send(StreamKind::Activity, &json!({
            "type":"activity", "lease_id":held.id, "request":{"frames":[{
                "fleet_response_chunk":{"text":text(sequence), "text_kind":"answer",
                    "stream_start":sequence == 0, "stream_contiguous":true, "stream_seq":sequence}
            }]}
        })).await;
        tail.observe(sequence).await?;
    }
    assert_eq!(
        tail.count(),
        40,
        "the complete activity path reached the live tail"
    );
    renew(native, &held, 5).await;
    report(fixture, native, &held).await
}

async fn renew(native: &mut Native, held: &Held, request_id: u64) {
    native
        .send(
            StreamKind::Control,
            &json!({
                "type":"renew_batch", "request_id":request_id,
                "leases":[{"lease_id":held.id, "request":{"requests":[]}}]
            }),
        )
        .await;
    let response = native.expect(StreamKind::Control, "renewed").await;
    assert_eq!(response.get("request_id"), Some(&json!(request_id)));
    assert_eq!(
        response.pointer("/leases/0/lease_id"),
        Some(&json!(held.id))
    );
    assert_eq!(
        response.pointer("/leases/0/answer/status"),
        Some(&json!("ok"))
    );
}

async fn report(fixture: &Fixture, native: &mut Native, held: &Held) -> Result<()> {
    let body = wire::report_body(&held.id, &fixture.run.event_id, held.fence);
    native
        .send(
            StreamKind::Reports,
            &json!({
                "type":"report", "request_id":6, "request":body
            }),
        )
        .await;
    let response = native.expect(StreamKind::Reports, "report_ack").await;
    assert_eq!(response.get("request_id"), Some(&json!(6)));
    assert_eq!(response.get("lease_id"), Some(&json!(held.id)));
    assert_eq!(
        response.pointer("/answer"),
        Some(&json!({"status":"ok","response":{"ok":true}}))
    );
    let settled: bool = sqlx::query_scalar(
        "SELECT settled_at IS NOT NULL FROM fleet.admissions WHERE id=$1 AND fleet_id=$2",
    )
    .bind(&fixture.run.event_id)
    .bind(&fixture.run.fleet)
    .fetch_one(&mut *fixture.owner.acquire().await?)
    .await?;
    assert!(
        settled,
        "the measured native lease reached durable settlement"
    );
    Ok(())
}

async fn measure_guard(fixture: &Fixture) -> Result<()> {
    let mut native = Native::connect(&fixture.run, Transport::H2).await;
    let measured = guarded(fixture, &mut native).await;
    native.close().await;
    measured
}

async fn guarded(fixture: &Fixture, native: &mut Native) -> Result<()> {
    healthy(native, 3).await;
    degrade(fixture).await?;
    let before = host_locks(&fixture.owner).await?;
    native.ready().await;
    attempted(fixture, before).await?;
    let next =
        tokio::time::timeout(Duration::from_millis(750), native.next(StreamKind::Work)).await;
    assert!(
        next.is_err(),
        "a stale healthy session must receive no offer"
    );
    assert_eq!(
        census(fixture).await?,
        (0, 1),
        "refused work remains an unsettled admission"
    );
    recovered(fixture, native).await
}

async fn degrade(fixture: &Fixture) -> Result<()> {
    sqlx::query("UPDATE runner.runners SET degraded_reason=$2 WHERE id=$1")
        .bind(&fixture.run.runner_id)
        .bind("a durable verdict changed after the healthy heartbeat")
        .execute(&mut *fixture.owner.acquire().await?)
        .await?;
    Ok(())
}

async fn recovered(fixture: &Fixture, native: &mut Native) -> Result<()> {
    healthy(native, 4).await;
    let degraded: bool =
        sqlx::query_scalar("SELECT degraded_reason IS NOT NULL FROM runner.runners WHERE id=$1")
            .bind(&fixture.run.runner_id)
            .fetch_one(&mut *fixture.owner.acquire().await?)
            .await?;
    assert!(
        !degraded,
        "the healthy heartbeat clears the durable degradation"
    );
    wake(fixture).await?;
    offer(&fixture.run, native).await?;
    assert_eq!(
        census(fixture).await?.0,
        1,
        "the same retained work can now be issued"
    );
    Ok(())
}

async fn attempted(fixture: &Fixture, before: i64) -> Result<()> {
    tokio::time::timeout(STREAM_ANSWER_TIMEOUT, async {
        loop {
            if host_locks(&fixture.owner).await? > before {
                return Ok::<(), Box<dyn std::error::Error + Send + Sync>>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await??;
    Ok(())
}

async fn census(fixture: &Fixture) -> Result<(i64, i64)> {
    Ok(sqlx::query_as(
        "SELECT \
        (SELECT count(*) FROM runner.leases WHERE runner_id=$1), \
        (SELECT count(*) FROM fleet.admissions WHERE id=$2 AND fleet_id=$3 AND settled_at IS NULL)",
    )
    .bind(&fixture.run.runner_id)
    .bind(&fixture.run.event_id)
    .bind(&fixture.run.fleet)
    .fetch_one(&mut *fixture.owner.acquire().await?)
    .await?)
}

async fn wake(fixture: &Fixture) -> Result<()> {
    let bus = fixture.run.booted.bus.clone();
    bus.publish(&Subject::Wake(0), fixture.run.fleet.as_str())
        .await?;
    bus.shutdown().await?;
    Ok(())
}
