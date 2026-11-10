//! Kill reaches a different daemon, and a lost command cannot preserve renewal authority.

use super::{TestResult, faults, fixture::Fixture, signals};
use crate::wire;
use af_core::{clock, id::Uuid7};
use af_stream::Transport;
use af_wire::protocol::StreamKind;
use serde_json::json;
use std::sync::atomic::Ordering;

// Value: protects=Kill commits cancellation and reaches the owning replica's native stream;
// fails_when=the operator mutation omits the marker/push or settlement loses the refusal class;
// why_new=the authoring store's publisher test has no gateway or native consumer;
// seam=two production daemons, authenticated HTTP, real Bus and Postgres.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_kill_on_another_replica_cancels_and_settles_the_native_lease() -> TestResult {
    for transport in [Transport::H2, Transport::H3] {
        let fixture = Fixture::start().await?;
        let mut native = fixture.native(transport).await;
        native.heartbeat().await;
        native.ready().await;
        let held = signals::offer(&mut native).await?;
        assert_eq!(held.event, fixture.run.event_id);
        fixture.operator.kill(&fixture.run).await?;
        signals::cancelled(&mut native, &held).await?;
        assert_marked(&fixture, &held).await?;
        signals::report(&fixture.run, &mut native, &held, true).await?;
        native.close().await;
        fixture.finish().await?;
    }
    Ok(())
}

// Value: protects=a lost cancel ends only its marked lease at the next native batch;
// fails_when=cancel authority depends on publication or the sibling loses its deadline;
// why_new=single-lease refusal tests cannot show independent sibling renewal;
// seam=real creator mutation and Postgres, with only its runner command dropped.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_lost_cancel_refuses_its_lease_and_renews_its_sibling_in_one_batch() -> TestResult {
    for transport in [Transport::H2, Transport::H3] {
        let mut fixture = Fixture::start().await?;
        let (_, sibling) = fixture.sibling("Pull request review").await;
        let mut native = fixture.native(transport).await;
        let held = pair(&fixture, &mut native).await?;
        let [cancelled, live] = &held;
        assert_eq!(cancelled.event, fixture.run.event_id);
        assert_eq!(live.event, sibling);
        lost_kill(&fixture).await?;
        assert_marked(&fixture, cancelled).await?;
        let answers = signals::renew(&mut native, &[cancelled, live], 4).await;
        assert_mixed(&answers, cancelled, live)?;
        signals::report(&fixture.run, &mut native, cancelled, true).await?;
        signals::report(&fixture.run, &mut native, live, false).await?;
        native.close().await;
        fixture.finish().await?;
    }
    Ok(())
}

async fn pair(
    fixture: &Fixture,
    native: &mut super::super::raw::Native,
) -> TestResult<[signals::Held; 2]> {
    native.heartbeat().await;
    native
        .send(
            StreamKind::Control,
            &json!({"type":"ready","credits":2,
        "revision":1,"observed_offers":0}),
        )
        .await;
    let mut held = [signals::offer(native).await?, signals::offer(native).await?];
    held.sort_by_key(|lease| lease.event != fixture.run.event_id);
    Ok(held)
}

async fn lost_kill(fixture: &Fixture) -> TestResult {
    let (bus, dropped) = faults::dropping(fixture.bus.clone(), fixture.run.runner_id.clone());
    let creations = fixture.creations(bus)?;
    let access = fixture.operator.access(&fixture.run);
    let current = creations.read(access, &fixture.run.fleet).await?;
    creations
        .patch(
            access,
            &fixture.run.fleet,
            Some(&current.etag),
            Some("killed"),
            None,
            clock::now(),
        )
        .await?;
    assert_eq!(
        dropped.load(Ordering::SeqCst),
        1,
        "the real Kill publication was lost"
    );
    Ok(())
}

async fn assert_marked(fixture: &Fixture, held: &signals::Held) -> TestResult {
    let marked: bool =
        sqlx::query_scalar("SELECT cancel_requested_at IS NOT NULL FROM runner.leases WHERE id=$1")
            .bind(Uuid7::parse(&held.lease)?)
            .fetch_one(&mut *fixture.run.booted.database.acquire().await?)
            .await?;
    assert!(
        marked,
        "native cancellation must have a committed database marker"
    );
    Ok(())
}

fn assert_mixed(
    answers: &serde_json::Value,
    cancelled: &signals::Held,
    live: &signals::Held,
) -> TestResult {
    let leases = wire::pointer(answers, "/leases")
        .as_array()
        .ok_or("renewal answers missing")?;
    assert_eq!(leases.len(), 2);
    let rejected = answer_for(leases, cancelled)?;
    assert_eq!(
        wire::pointer(rejected, "/answer/error_code"),
        af_core::error_code::RUN_LEASE_CANCELLED.as_str()
    );
    assert_eq!(wire::pointer(rejected, "/answer/retryable"), false);
    let accepted = answer_for(leases, live)?;
    assert_eq!(wire::pointer(accepted, "/answer/status"), "ok");
    let expires: jiff::Timestamp = wire::pointer(accepted, "/answer/response/lease_expires_at")
        .as_str()
        .ok_or("sibling deadline missing")?
        .parse()?;
    assert!(
        expires > clock::now(),
        "the unmarked sibling retains live authority"
    );
    Ok(())
}

fn answer_for<'a>(
    leases: &'a [serde_json::Value],
    held: &signals::Held,
) -> TestResult<&'a serde_json::Value> {
    Ok(leases
        .iter()
        .find(|row| wire::pointer(row, "/lease_id") == held.lease.as_str())
        .ok_or("lease answer missing")?)
}
