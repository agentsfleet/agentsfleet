//! Stale native issue and renewal operations cannot regain authority after a host-lock wait.

use super::super::raw::Native;
use super::{
    TestResult,
    fixture::Fixture,
    locks::{self, HostLock, Replacement},
    signals,
};
use crate::wire;
use af_stream::Transport;
use af_wire::protocol::StreamKind;
use serde_json::json;

// Value: protects=the host lock rechecks credential and connection after waiting;
// fails_when=an old native claim issues work or an old renewal advances expiry;
// why_new=capacity-store tests bypass the native gateway and claim owner;
// seam=real daemons and Postgres locks; direct row replacement models lost notifications.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn stale_native_grants_cannot_survive_rotation_or_supersession_at_the_host_lock() -> TestResult
{
    for transport in [Transport::H2, Transport::H3] {
        for replacement in [Replacement::Rotate, Replacement::Supersede] {
            let fixture = Fixture::start().await?;
            stale_claim(&fixture, transport, replacement).await?;
            fixture.finish().await?;
            let fixture = Fixture::start().await?;
            stale_renewal(&fixture, transport, replacement).await?;
            fixture.finish().await?;
        }
    }
    Ok(())
}

async fn stale_claim(
    fixture: &Fixture,
    transport: Transport,
    replacement: Replacement,
) -> TestResult {
    let mut old = fixture.native(transport).await;
    old.heartbeat().await;
    let locked = HostLock::hold(&fixture.run).await?;
    old.ready().await;
    let waiter = locked.blocked(&fixture.run).await?;
    let token = locked.replace(&fixture.run, replacement).await?;
    locks::finished(&fixture.run, waiter).await?;
    assert_unissued(fixture, replacement).await?;
    let mut current =
        Native::connect_at(fixture.peer.runner_stream_address, &token, transport).await;
    current.heartbeat().await;
    signals::no_offer(&mut old).await?;
    current.ready().await;
    let held = signals::offer(&mut current).await?;
    assert_eq!(held.event, fixture.run.event_id);
    signals::report(&fixture.run, &mut current, &held, false).await?;
    current.close().await;
    old.close().await;
    Ok(())
}

async fn assert_unissued(fixture: &Fixture, replacement: Replacement) -> TestResult {
    let issued: i64 = sqlx::query_scalar("SELECT count(*) FROM runner.leases WHERE event_id=$1")
        .bind(&fixture.run.event_id)
        .fetch_one(&mut *fixture.run.booted.database.acquire().await?)
        .await?;
    assert_eq!(issued, 0, "a stale issue wrote a lease: {replacement:?}");
    let pending: bool =
        sqlx::query_scalar("SELECT settled_at IS NULL FROM fleet.admissions WHERE id=$1")
            .bind(&fixture.run.event_id)
            .fetch_one(&mut *fixture.run.booted.database.acquire().await?)
            .await?;
    assert!(pending, "refused issuance must retain the admitted work");
    Ok(())
}

async fn stale_renewal(
    fixture: &Fixture,
    transport: Transport,
    replacement: Replacement,
) -> TestResult {
    let mut old = fixture.native(transport).await;
    old.heartbeat().await;
    old.ready().await;
    let held = signals::offer(&mut old).await?;
    let locked = HostLock::hold(&fixture.run).await?;
    old.send(
        StreamKind::Control,
        &json!({"type":"renew_batch","request_id":4,
        "leases":[{"lease_id":held.lease,"request":{"requests":[]}}]}),
    )
    .await;
    locked.blocked(&fixture.run).await?;
    let token = locked.replace(&fixture.run, replacement).await?;
    assert_stale_renewal(fixture, &mut old, &held).await?;
    let mut current =
        Native::connect_at(fixture.peer.runner_stream_address, &token, transport).await;
    let renewed = signals::renew(&mut current, &[&held], 4).await;
    assert_eq!(wire::pointer(&renewed, "/leases/0/answer/status"), "ok");
    signals::report(&fixture.run, &mut current, &held, false).await?;
    current.close().await;
    old.close().await;
    Ok(())
}

async fn assert_stale_renewal(
    fixture: &Fixture,
    old: &mut Native,
    held: &signals::Held,
) -> TestResult {
    let denied = old.expect(StreamKind::Control, "renewed").await;
    assert_eq!(
        wire::pointer(&denied, "/leases/0/answer/error_code"),
        af_core::error_code::RUN_LEASE_LOST.as_str()
    );
    let expiry: jiff_sqlx::Timestamp =
        sqlx::query_scalar("SELECT lease_expires_at FROM runner.leases WHERE id=$1")
            .bind(af_core::id::Uuid7::parse(&held.lease)?)
            .fetch_one(&mut *fixture.run.booted.database.acquire().await?)
            .await?;
    assert_eq!(
        expiry.to_jiff(),
        held.expires,
        "a stale renewal advanced authority"
    );
    Ok(())
}
