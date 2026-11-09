//! Operator changes close stale native owners; durable reads cover a lost notification.

use super::super::raw::Native;
use super::{
    TestResult, faults,
    fixture::Fixture,
    locks::{self, HostLock},
    signals,
};
use crate::wire;
use af_core::{
    clock,
    error_code::{RUN_ADMIN_STATE_BLOCKED, RUN_INVALID_RUNNER_TOKEN, RUN_STREAM_SUPERSEDED},
};
use af_crypto::entropy::Entropy;
use af_stream::Transport;
use af_wire::protocol::StreamKind;
use afd_bus::{Command, Subject};
use serde_json::json;
use std::sync::atomic::Ordering;

// Value: protects=real operator revocation and rotation push coded closes to another replica;
// fails_when=the postcommit publisher is omitted, old credentials reopen, or a rotated key is unusable;
// why_new=direct database backstop tests cannot detect a missing operator publication;
// seam=authenticated operator HTTP, real Bus/Postgres and both native carriers.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn operator_revocation_and_rotation_close_the_previous_native_owner() -> TestResult {
    for transport in [Transport::H2, Transport::H3] {
        for action in ["revoke", "rotate"] {
            let fixture = Fixture::start().await?;
            operator_transition(&fixture, transport, action).await?;
            fixture.finish().await?;
        }
    }
    Ok(())
}

async fn operator_transition(fixture: &Fixture, transport: Transport, action: &str) -> TestResult {
    let mut native = fixture.native(transport).await;
    native.heartbeat().await;
    let answer = fixture.operator.runner(&fixture.run, action).await?;
    let code = if action == "revoke" {
        RUN_ADMIN_STATE_BLOCKED
    } else {
        RUN_INVALID_RUNNER_TOKEN
    };
    signals::away(&mut native, code).await?;
    signals::no_offer(&mut native).await?;
    signals::opening_refused(&fixture.run, &fixture.run.token, code, transport).await?;
    if action == "rotate" {
        rotated(fixture, &answer, transport).await?;
    }
    native.close().await;
    Ok(())
}

async fn rotated(
    fixture: &Fixture,
    answer: &serde_json::Value,
    transport: Transport,
) -> TestResult {
    let token = wire::pointer(answer, "/runner_token")
        .as_str()
        .ok_or("replacement token missing")?;
    let mut current =
        Native::connect_at(fixture.peer.runner_stream_address, token, transport).await;
    current.heartbeat().await;
    current.ready().await;
    let held = signals::offer(&mut current).await?;
    assert_eq!(held.event, fixture.run.event_id);
    signals::report(&fixture.run, &mut current, &held, false).await?;
    current.close().await;
    Ok(())
}

// Value: protects=a lost revocation push cannot issue work and closes at the next heartbeat;
// fails_when=authority comes from cached policy or heartbeat ignores the committed revoke;
// why_new=successful pushes cannot exercise the Postgres backstop;
// seam=the real runner transition and a dropped runner command, then native issue/heartbeat.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_lost_revocation_push_is_fenced_at_issue_and_the_next_native_heartbeat() -> TestResult {
    for transport in [Transport::H2, Transport::H3] {
        let fixture = Fixture::start().await?;
        let mut native = fixture.native(transport).await;
        native.heartbeat().await;
        revoke_without_delivery(&fixture).await?;
        let held = HostLock::hold(&fixture.run).await?;
        native.ready().await;
        let waiter = held.blocked(&fixture.run).await?;
        held.release().await?;
        locks::finished(&fixture.run, waiter).await?;
        let leases: i64 =
            sqlx::query_scalar("SELECT count(*) FROM runner.leases WHERE event_id=$1")
                .bind(&fixture.run.event_id)
                .fetch_one(&mut *fixture.run.booted.database.acquire().await?)
                .await?;
        assert_eq!(leases, 0, "revoked cached capacity must not issue a lease");
        heartbeat_refused(&mut native).await?;
        signals::no_offer(&mut native).await?;
        native.close().await;
        fixture.finish().await?;
    }
    Ok(())
}

async fn revoke_without_delivery(fixture: &Fixture) -> TestResult {
    afd_runner::Runners::new(fixture.run.booted.database.clone(), Entropy::new())
        .transition(
            &fixture.run.runner_id,
            serde_json::from_value(json!("revoke"))?,
            crate::e2e::ACTOR,
            clock::now(),
        )
        .await?;
    let (bus, dropped) = faults::dropping(fixture.bus.clone(), fixture.run.runner_id.clone());
    bus.publish_frame(
        &Subject::RunnerCommand(fixture.run.runner_id.clone()),
        &Command::Recheck,
    )
    .await?;
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    Ok(())
}

async fn heartbeat_refused(native: &mut Native) -> TestResult {
    native
        .send(
            StreamKind::Control,
            &json!({"type":"heartbeat","request_id":4,
        "request":wire::capable_beat()}),
        )
        .await;
    let refused = native.expect(StreamKind::Control, "heartbeat").await;
    assert_eq!(
        wire::pointer(&refused, "/answer/error_code"),
        RUN_ADMIN_STATE_BLOCKED.as_str()
    );
    assert_eq!(wire::pointer(&refused, "/answer/retryable"), false);
    signals::away(native, RUN_ADMIN_STATE_BLOCKED).await
}

// Value: protects=one committed owner receives offers across replica registries;
// fails_when=ownership publication is local-only or stale credits receive fresh work;
// why_new=the existing supersession case connects twice to one replica;
// seam=two production daemons sharing the actual Bus and Postgres.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_new_replica_supersedes_the_old_native_owner_and_alone_receives_offers() -> TestResult {
    for transport in [Transport::H2, Transport::H3] {
        let fixture = Fixture::start().await?;
        status(&fixture, "stopped").await?;
        let mut old = Native::connect(&fixture.run, transport).await;
        old.heartbeat().await;
        old.ready().await;
        let mut current = fixture.native(transport).await;
        assert_ne!(old.owner, current.owner);
        signals::away(&mut old, RUN_STREAM_SUPERSEDED).await?;
        signals::no_offer(&mut old).await?;
        status(&fixture, "active").await?;
        current.heartbeat().await;
        current.ready().await;
        let held = signals::offer(&mut current).await?;
        assert_eq!(held.event, fixture.run.event_id);
        signals::report(&fixture.run, &mut current, &held, false).await?;
        current.close().await;
        old.close().await;
        fixture.finish().await?;
    }
    Ok(())
}

async fn status(fixture: &Fixture, status: &str) -> TestResult {
    sqlx::query("UPDATE fleet.fleets SET status=$2 WHERE id=$1")
        .bind(&fixture.run.fleet)
        .bind(status)
        .execute(&mut *fixture.run.booted.database.acquire().await?)
        .await?;
    Ok(())
}
