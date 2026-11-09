//! Lost renewal answers retain independent per-lease meter cursors across real native recovery.
#![cfg(all(feature = "test-util", target_os = "linux"))]
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

mod fixture;
mod reads;
mod relay;

use super::recovery::proxy::FaultProxy;
use crate::{e2e::scenario, wire};
use af_core::timing::{RENEWAL_TICK, STREAM_ANSWER_TIMEOUT, STREAM_RECOVERY_BUDGET};
use af_stream::Transport;
use af_wire::usage::{LINES_MAX, RequestLines};
use afr_agent::Meter;
use afr_supervisor::test_util::Session;
use agentsfleetd::supervisor::Supervisor;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

const FIRST: usize = LINES_MAX + 13;
const SECOND: usize = LINES_MAX + 8 + 29;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn renewal_batches_acknowledge_usage_independently_across_reconnect() {
    for carrier in [Transport::H2, Transport::H3] {
        let mut supervisor = Supervisor::new();
        let run = scenario(&mut supervisor).await;
        let sibling = fixture::sibling(&run).await;
        let relay = relay::Relay::start(run.booted.runner_stream_address).await;
        let proxy = FaultProxy::start(relay.address, carrier).await;
        let mut session = Arc::new(
            Session::connect(&fixture::config(&run, &proxy.origin, &relay.root()))
                .await
                .expect("production native downlink"),
        );
        session.capacity(2);
        let beat = serde_json::to_vec(&wire::capable_beat()).expect("heartbeat JSON");
        session
            .heartbeat(&serde_json::from_slice(&beat).expect("heartbeat body"))
            .await
            .expect("capable heartbeat");
        let held = fixture::offers(&session, &run.event_id, &sibling.1).await;
        proxy.assert_initial_carrier(carrier);
        replay(&run, &relay, &session, &held).await;
        Arc::get_mut(&mut session)
            .expect("renewal owners joined")
            .close()
            .await
            .expect("native cleanup");
        proxy.close().await;
        relay.close().await;
        supervisor.shutdown().await;
        crate::e2e_retire::retire_fleet(&run.booted, &sibling.0).await;
        run.cleanup().await;
    }
}

async fn replay(
    run: &crate::e2e::Scenario,
    relay: &relay::Relay,
    session: &Arc<Session>,
    held: &[fixture::Held; 2],
) {
    let meters = [fixture::meter(FIRST, 100), fixture::meter(SECOND, 200)];
    // Start the sibling at a different cursor using a real acknowledged renewal.
    let prefix = RequestLines(meters[1].unrecorded().0.into_iter().take(8).collect());
    let warm = session
        .renew(
            &held[1].lease,
            &af_wire::report::RenewRequest { requests: prefix },
        )
        .await
        .expect("warm sibling cursor");
    assert_eq!(warm.recorded_through, Some(7));
    meters[1].recorded_through(7);
    let before = reads::owner(run).await;
    relay.lose_next_renewal();
    let stop = CancellationToken::new();
    let _stop_on_exit = stop.clone().drop_guard();
    let jobs = fixture::keep(session, held, &meters, &stop);
    reads::wait_lines(run, held, [LINES_MAX, LINES_MAX + 8]).await;
    assert_eq!(
        [meters[0].backlog(), meters[1].backlog()],
        [FIRST, SECOND - 8],
        "committed answers were lost before meters advanced"
    );
    relay.lost().await;
    relay.resume();
    reads::changed_owner(run, &before).await;
    acknowledged(&meters).await;
    stop.cancel();
    for job in jobs {
        job.await.expect("renewal owner joins");
    }
    final_reports(session, held, &meters).await;
    reads::settled(run, held, [FIRST, SECOND]).await;
}

async fn final_reports(session: &Session, held: &[fixture::Held; 2], meters: &[Arc<Meter>; 2]) {
    let mut first = wire::report_body(held[0].lease.as_str(), &held[0].event, held[0].fence);
    let mut second = wire::report_body(held[1].lease.as_str(), &held[1].event, held[1].fence);
    (*first
        .pointer_mut("/requests")
        .expect("native response field")) =
        serde_json::to_value(meters[0].unrecorded()).expect("first residual usage");
    (*second
        .pointer_mut("/requests")
        .expect("native response field")) =
        serde_json::to_value(meters[1].unrecorded()).expect("second residual usage");
    for body in [first, second] {
        let bytes = serde_json::to_vec(&body).expect("current report bytes");
        for _ in 0..2 {
            assert!(
                session
                    .report(&bytes)
                    .await
                    .expect("final native report")
                    .ok
            );
        }
    }
}

async fn acknowledged(meters: &[Arc<Meter>; 2]) {
    tokio::time::timeout(
        STREAM_ANSWER_TIMEOUT + STREAM_RECOVERY_BUDGET + RENEWAL_TICK,
        async {
            loop {
                if [meters[0].backlog(), meters[1].backlog()] == [13, 29] {
                    return;
                }
                tokio::task::yield_now().await;
            }
        },
    )
    .await
    .expect("each lease acknowledges its own highest saved sequence after replay");
}
