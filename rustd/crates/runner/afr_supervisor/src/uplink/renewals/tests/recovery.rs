//! Reconnection must renew live work before its independent cleanup deadline.

use super::RenewalPeer;
use crate::{
    client::{ControlPlane, HttpRunnerApi},
    config::Config,
    renew::Renewal,
};
use af_core::{
    clock::FixedClock,
    env::MapEnv,
    id::Uuid7,
    timing::{LEASE_TTL, RENEWAL_TICK, STREAM_ANSWER_TIMEOUT},
};
use af_wire::protocol::Up;
use futures_util::poll;
use std::{pin::Pin, sync::Arc, time::Duration};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

#[tokio::test(start_paused = true)]
async fn reconnect_renews_before_the_next_periodic_tick() -> crate::Result<()> {
    recovered(Duration::from_secs(17), Duration::from_secs(2)).await
}

#[tokio::test(start_paused = true)]
async fn republishing_the_same_connection_does_not_start_an_extra_renewal() -> crate::Result<()> {
    let (handle, mut peer) = RenewalPeer::new();
    let registry = Arc::clone(&handle.renewing);
    let lease = Uuid7::parse(crate::test_util::LEASE_ID)?;
    let _held = registry.register(
        &lease,
        Arc::new(afr_agent::Meter::default()),
        Instant::now() + LEASE_TTL,
    )?;
    let stop = CancellationToken::new();
    let renewing = registry.run(&handle, stop.clone());
    tokio::pin!(renewing);
    assert!(poll!(&mut renewing).is_pending());
    peer._current.send_replace(Some(Arc::clone(&peer.sender)));
    assert!(poll!(&mut renewing).is_pending());
    assert!(matches!(
        peer.outgoing.priority.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));
    tokio::time::advance(RENEWAL_TICK).await;
    assert!(poll!(&mut renewing).is_pending());
    assert!(
        peer.outgoing.priority.try_recv().is_ok(),
        "the ordinary renewal tick still runs"
    );
    stop.cancel();
    renewing.await
}

#[tokio::test(start_paused = true)]
async fn the_full_recovery_and_answer_budgets_preserve_running_work() -> crate::Result<()> {
    let connected = RENEWAL_TICK
        + STREAM_ANSWER_TIMEOUT
        + af_core::timing::RECONNECT_JITTER_MAX
        + af_core::timing::STREAM_RECOVERY_BUDGET;
    recovered(connected, STREAM_ANSWER_TIMEOUT).await
}

async fn recovered(connected: Duration, answer_delay: Duration) -> crate::Result<()> {
    let (handle, mut old) = RenewalPeer::new();
    let plane = control_plane(handle.clone())?;
    let lease = Uuid7::parse(crate::test_util::LEASE_ID)?;
    let clock = FixedClock::at(jiff::Timestamp::UNIX_EPOCH);
    let meter = Arc::new(afr_agent::Meter::default());
    let grant = af_core::clock::after(jiff::Timestamp::UNIX_EPOCH, LEASE_TTL);
    let keeping = Renewal::new(&plane, &lease, grant, &clock, &meter).keep();
    let stop = CancellationToken::new();
    let registry = Arc::clone(&handle.renewing);
    let renewing = registry.run(&handle, stop.clone());
    tokio::pin!(keeping, renewing);
    assert!(poll!(&mut keeping).is_pending());
    assert!(poll!(&mut renewing).is_pending());
    let started = Instant::now();
    lose_first_renewal(renewing.as_mut(), &mut old).await?;
    tokio::time::advance(connected - started.elapsed()).await;
    assert!(poll!(&mut renewing).is_pending());
    let (_, mut replacement) = RenewalPeer::new();
    old._current
        .send_replace(Some(Arc::clone(&replacement.sender)));
    assert!(poll!(&mut renewing).is_pending());
    let immediate = replacement.outgoing.priority.try_recv();
    assert!(
        immediate.is_ok(),
        "reconnect must renew without waiting for another periodic tick"
    );
    let message = immediate.map_err(crate::error::stream_protocol)?;
    let Up::RenewBatch { request_id, .. } = message.frame.decode::<Up<'_>>()? else {
        return Err(crate::error::ErrorKind::StreamProtocol.into());
    };
    tokio::time::advance(answer_delay).await;
    assert!(
        poll!(&mut keeping).is_pending(),
        "a permitted recovery must leave the complete answer budget before cleanup"
    );
    replacement
        .deliver(
            &serde_json::json!({"type":"renewed", "request_id":request_id,
        "leases":[{"lease_id":lease, "answer":{"status":"ok", "response":{
            "lease_expires_at":"2030-01-01T00:00:00Z"}}}]}),
        )
        .await?;
    assert!(poll!(&mut renewing).is_pending());
    assert!(poll!(&mut keeping).is_pending());
    stop.cancel();
    renewing.await?;
    Ok(())
}

async fn lose_first_renewal(
    mut renewing: Pin<&mut impl Future<Output = crate::Result<()>>>,
    old: &mut RenewalPeer,
) -> crate::Result<()> {
    tokio::time::advance(RENEWAL_TICK).await;
    assert!(poll!(&mut renewing).is_pending());
    let unanswered = old
        .outgoing
        .priority
        .try_recv()
        .map_err(crate::error::stream_protocol)?;
    assert!(matches!(
        unanswered.frame.decode::<Up<'_>>()?,
        Up::RenewBatch { .. }
    ));
    tokio::time::advance(STREAM_ANSWER_TIMEOUT).await;
    assert!(poll!(&mut renewing).is_pending());
    old.sender.stop.cancel();
    old.sender.requests.close();
    old._current.send_replace(None);
    Ok(())
}

fn control_plane(handle: crate::uplink::StreamHandle) -> crate::Result<ControlPlane> {
    let config = Config::from_env(&MapEnv::from_pairs([
        (crate::config::ENV_API_URL, "http://127.0.0.1:1"),
        (crate::config::ENV_STREAM_URL, "https://localhost:4443"),
        (crate::config::ENV_RUNNER_TOKEN, "agt_r_fixture"),
    ]))?;
    Ok(ControlPlane::new(
        Box::new(HttpRunnerApi::new(&config)?),
        handle,
    ))
}
