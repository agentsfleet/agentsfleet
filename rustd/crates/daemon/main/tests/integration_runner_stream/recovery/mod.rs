//! Real runner work survives a bounded lost carrier and stops at expiry during a sustained outage.
#![cfg(all(feature = "test-util", target_os = "linux"))]

pub(super) mod proxy;
mod reads;
mod runner;

use af_core::timing::{RECONNECT_JITTER_MAX, STREAM_ANSWER_TIMEOUT, STREAM_RECOVERY_BUDGET};
use af_stream::Transport;
use agentsfleetd::supervisor::Supervisor;
use std::time::Duration;
use tokio::time::Instant;

use crate::e2e::scenario;
use proxy::FaultProxy;
use runner::Runner;

const CUT: Duration = STREAM_ANSWER_TIMEOUT.saturating_add(Duration::from_secs(1));

// Value: protects=recovery confirms a new grant before the running lease expires;
// fails_when=the database renews but the runner loses its answer or ends at the old deadline;
// why_new=the retained-report case completes its engine before it needs another grant;
// seam=real production runner, physical process, faulted carriers and Postgres.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_bounded_native_outage_renews_before_its_confirmed_deadline() {
    for carrier in [Transport::H2, Transport::H3] {
        renewed_before_expiry(carrier).await;
    }
}

async fn renewed_before_expiry(carrier: Transport) {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let proxy = FaultProxy::start(run.booted.runner_stream_address, carrier).await;
    let worker = Runner::start(&run, &proxy.origin).await;
    let pid = worker.started().await;
    proxy.assert_initial_carrier(carrier);
    let before = reads::snapshot(&run).await;
    let dropped = proxy.dropped_bytes();
    let interrupted = Instant::now();
    proxy.blackhole();
    tokio::time::sleep(CUT).await;
    assert!(proxy.dropped_bytes() > dropped);
    proxy.restore_new_connections();
    let budget = STREAM_ANSWER_TIMEOUT + RECONNECT_JITTER_MAX + STREAM_RECOVERY_BUDGET;
    let renewed = reads::wait_renewed_before(&run, &before, budget).await;
    assert!(interrupted.elapsed() < CUT + budget);
    assert_ne!(renewed.owner, before.owner);
    assert_eq!(renewed.lease, before.lease);
    reads::process_survives_deadline(pid, before.expires_at).await;
    assert!(!reads::reported(&run, &before.lease).await);
    worker.release();
    reads::wait_reported(&run, &before.lease, budget).await;
    assert_eq!(worker.executions(), 1);
    reads::assert_settled_once(&run).await;
    worker.stop().await;
    proxy.close().await;
    supervisor.shutdown().await;
    run.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_blackholed_native_runner_reconnects_and_settles_its_memory_report_once() {
    for carrier in [Transport::H2, Transport::H3] {
        bounded(carrier).await;
    }
}

async fn bounded(carrier: Transport) {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let proxy = FaultProxy::start(run.booted.runner_stream_address, carrier).await;
    let worker = Runner::start(&run, &proxy.origin).await;
    worker.started().await;
    proxy.assert_initial_carrier(carrier);
    let before = reads::snapshot(&run).await;
    let dropped = proxy.dropped_bytes();
    let interrupted = Instant::now();
    proxy.blackhole();
    worker.release();
    tokio::time::sleep(CUT).await;
    assert!(
        proxy.dropped_bytes() > dropped,
        "the established carrier actually lost bytes"
    );
    assert!(
        !reads::reported(&run, &before.lease).await,
        "a report cannot settle through the blackhole"
    );
    proxy.restore_new_connections();
    let budget = STREAM_ANSWER_TIMEOUT + RECONNECT_JITTER_MAX + STREAM_RECOVERY_BUDGET;
    reads::wait_reported(&run, &before.lease, budget).await;
    assert!(interrupted.elapsed() < CUT + budget);
    let recovered = reads::snapshot(&run).await;
    assert_ne!(
        recovered.owner, before.owner,
        "the runner recovered a new authenticated session"
    );
    assert_eq!(recovered.lease, before.lease);
    assert_eq!(worker.executions(), 1);
    reads::assert_settled_once(&run).await;
    worker.stop().await;
    proxy.close().await;
    supervisor.shutdown().await;
    run.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_sustained_native_outage_never_extends_the_running_process_deadline() {
    for carrier in [Transport::H2, Transport::H3] {
        sustained(carrier).await;
    }
}

async fn sustained(carrier: Transport) {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let proxy = FaultProxy::start(run.booted.runner_stream_address, carrier).await;
    let worker = Runner::start(&run, &proxy.origin).await;
    let pid = worker.started().await;
    proxy.assert_initial_carrier(carrier);
    let dropped = proxy.dropped_bytes();
    proxy.blackhole();
    tokio::time::sleep(STREAM_ANSWER_TIMEOUT).await;
    let last_possible_confirmation = reads::snapshot(&run).await;
    reads::process_stopped_by(pid, last_possible_confirmation.expires_at).await;
    assert!(
        proxy.dropped_bytes() > dropped,
        "the established carrier actually lost bytes"
    );
    assert!(!reads::reported(&run, &last_possible_confirmation.lease).await);
    assert_eq!(worker.executions(), 1);
    worker.stop().await;
    proxy.close().await;
    supervisor.shutdown().await;
    run.cleanup().await;
}
