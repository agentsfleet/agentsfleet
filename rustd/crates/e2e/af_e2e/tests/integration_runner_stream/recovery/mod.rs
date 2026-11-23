//! Real runner work survives a bounded lost carrier and stops at expiry during a sustained outage.
#![cfg(target_os = "linux")]

pub(super) mod proxy;
mod reads;
mod runner;

use af_core::timing::{STREAM_ANSWER_TIMEOUT, STREAM_RECOVERY_BUDGET};
use af_stream::Transport;
use agentsfleetd::supervisor::Supervisor;
use std::time::Duration;
use tokio::time::Instant;

use super::RECOVERY_SLACK;
use crate::e2e::scenario;
use proxy::FaultProxy;
use runner::Runner;

const CUT: Duration = STREAM_ANSWER_TIMEOUT.saturating_add(Duration::from_secs(1));

// Value: protects=recovery confirms a new grant before the running run expires;
// fails_when=the database renews but the runner loses its answer or ends at the old deadline;
// why_new=the retained-report case completes its engine before it needs another grant;
// seam=real production runner, physical process, faulted carriers and Postgres.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_bounded_native_outage_renews_before_its_confirmed_deadline() {
    for carrier in [Transport::H2, Transport::H3] {
        renewed_before_expiry(carrier).await;
    }
}

async fn renewed_before_expiry(carrier: Transport) {
    let mut supervisor = Supervisor::new();
    let scene = scenario(&mut supervisor).await;
    let proxy = FaultProxy::start(scene.booted.runner_stream_address, carrier).await;
    let worker = Runner::start(&scene, &proxy.origin).await;
    let pid = worker.started().await;
    proxy.assert_initial_carrier(carrier);
    let before = reads::snapshot(&scene).await;
    let dropped = proxy.dropped_bytes();
    let interrupted = Instant::now();
    proxy.blackhole();
    tokio::time::sleep(CUT).await;
    assert!(proxy.dropped_bytes() > dropped);
    proxy.restore_new_connections();
    let budget = STREAM_ANSWER_TIMEOUT + RECOVERY_SLACK + STREAM_RECOVERY_BUDGET;
    let renewed = reads::wait_renewed_before(&scene, &before, budget).await;
    assert!(interrupted.elapsed() < CUT + budget);
    assert_ne!(renewed.owner, before.owner);
    assert_eq!(renewed.run, before.run);
    reads::process_survives_deadline(pid, before.expires_at).await;
    assert!(!reads::reported(&scene, &before.run).await);
    worker.release();
    reads::wait_reported(&scene, &before.run, budget).await;
    assert_eq!(worker.executions(), 1);
    reads::assert_settled_once(&scene).await;
    worker.stop().await;
    proxy.close().await;
    supervisor.shutdown().await;
    scene.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_blackholed_native_runner_reconnects_and_settles_its_memory_report_once() {
    for carrier in [Transport::H2, Transport::H3] {
        bounded(carrier).await;
    }
}

async fn bounded(carrier: Transport) {
    let mut supervisor = Supervisor::new();
    let scene = scenario(&mut supervisor).await;
    let proxy = FaultProxy::start(scene.booted.runner_stream_address, carrier).await;
    let worker = Runner::start(&scene, &proxy.origin).await;
    worker.started().await;
    proxy.assert_initial_carrier(carrier);
    let before = reads::snapshot(&scene).await;
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
        !reads::reported(&scene, &before.run).await,
        "a report cannot settle through the blackhole"
    );
    proxy.restore_new_connections();
    let budget = STREAM_ANSWER_TIMEOUT + RECOVERY_SLACK + STREAM_RECOVERY_BUDGET;
    reads::wait_reported(&scene, &before.run, budget).await;
    assert!(interrupted.elapsed() < CUT + budget);
    let recovered = reads::snapshot(&scene).await;
    assert_ne!(
        recovered.owner, before.owner,
        "the runner recovered a new authenticated session"
    );
    assert_eq!(recovered.run, before.run);
    assert_eq!(worker.executions(), 1);
    reads::assert_settled_once(&scene).await;
    worker.stop().await;
    proxy.close().await;
    supervisor.shutdown().await;
    scene.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_sustained_native_outage_never_extends_the_running_process_deadline() {
    for carrier in [Transport::H2, Transport::H3] {
        sustained(carrier).await;
    }
}

async fn sustained(carrier: Transport) {
    let mut supervisor = Supervisor::new();
    let scene = scenario(&mut supervisor).await;
    let proxy = FaultProxy::start(scene.booted.runner_stream_address, carrier).await;
    let worker = Runner::start(&scene, &proxy.origin).await;
    let pid = worker.started().await;
    proxy.assert_initial_carrier(carrier);
    let dropped = proxy.dropped_bytes();
    proxy.blackhole();
    tokio::time::sleep(STREAM_ANSWER_TIMEOUT).await;
    let last_possible_confirmation = reads::snapshot(&scene).await;
    reads::process_stopped_by(pid, last_possible_confirmation.expires_at).await;
    assert!(
        proxy.dropped_bytes() > dropped,
        "the established carrier actually lost bytes"
    );
    assert!(!reads::reported(&scene, &last_possible_confirmation.run).await);
    assert_eq!(worker.executions(), 1);
    worker.stop().await;
    proxy.close().await;
    supervisor.shutdown().await;
    scene.cleanup().await;
}
