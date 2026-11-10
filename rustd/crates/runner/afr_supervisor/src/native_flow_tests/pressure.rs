//! Bulk report and activity pressure cannot obstruct control acknowledgements or cancellation.

use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use af_core::{env::MapEnv, id::Uuid7, test_util::trace::Capture};
use af_stream::Transport;
use af_wire::activity::{ActivityFrame, ActivityRequest, ToolCallStarted};
use futures_util::poll;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::{TestResult, kind};
use crate::{
    config::{Config, ENV_API_URL, ENV_RUNNER_TOKEN, ENV_STREAM_CA, ENV_STREAM_URL},
    test_support::{FLEET_ID, LEASE_ID, healthy, lease, report_body},
    test_util::{NativeDaemon, Session},
};

const REPORT_BYTES: usize = 1024 * 1024;
const ACTIVITY_BYTES: usize = 128 * 1024;

#[derive(Default)]
struct Blocked {
    report: CancellationToken,
    activity: CancellationToken,
    release: CancellationToken,
    report_bytes: AtomicUsize,
    completed: AtomicUsize,
}

impl Blocked {
    async fn observe(&self, frame: Value) {
        match kind(&frame) {
            Some("report") => {
                let bytes = frame
                    .pointer("/request/response_text")
                    .and_then(Value::as_str)
                    .map_or(0, str::len);
                self.report_bytes.store(bytes, Ordering::SeqCst);
                self.report.cancel();
            }
            Some("activity") => self.activity.cancel(),
            _ => return,
        }
        self.release.cancelled().await;
        self.completed.fetch_add(1, Ordering::SeqCst);
    }

    fn assert_pending(&self) {
        assert!(
            self.report.is_cancelled(),
            "the large report reached the peer"
        );
        assert!(
            self.activity.is_cancelled(),
            "activity reached its blocked reader"
        );
        assert!(!self.release.is_cancelled(), "bulk readers remain blocked");
        assert_eq!(self.report_bytes.load(Ordering::SeqCst), REPORT_BYTES);
        assert_eq!(self.completed.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn large_report_and_saturated_activity_leave_renewal_and_cancel_independent() -> TestResult {
    let logs = Capture::install();
    for transport in [Transport::H3, Transport::H2] {
        exercise(transport, &logs).await?;
    }
    Ok(())
}

async fn exercise(transport: Transport, logs: &Capture) -> TestResult {
    let blocked = Arc::new(Blocked::default());
    let observed = Arc::clone(&blocked);
    let _release_on_exit = blocked.release.clone().drop_guard();
    let payload = serde_json::to_value(af_wire::lease::TaggedLease::<Value>::Fleet(Box::new(
        lease(LEASE_ID, FLEET_ID),
    )))?;
    let mut daemon = NativeDaemon::serve(healthy(), payload, move |frame| {
        let blocked = Arc::clone(&observed);
        async move { blocked.observe(frame).await }
    })
    .await?;
    let relay = match transport {
        Transport::H2 => Some(daemon.tcp_only().await?),
        Transport::H3 => None,
    };
    let origin = relay
        .as_ref()
        .map_or(daemon.url.as_str(), |relay| relay.url.as_str());
    let connected_after = logs.events().len();
    let mut session = Session::connect(&configured(&daemon, origin)?).await?;
    session.capacity(1);
    let offer = tokio::time::timeout(Duration::from_secs(5), session.offer()).await??;
    assert_ne!(offer.as_bytes(), b"");
    assert_carrier(logs, connected_after, transport);
    simultaneous(&session, &daemon, &blocked, logs).await?;
    session.close().await?;
    if let Some(relay) = relay {
        relay.close().await?;
    }
    daemon.close().await?;
    Ok(())
}

fn configured(daemon: &NativeDaemon, origin: &str) -> TestResult<Config> {
    let root = daemon.root();
    Ok(Config::from_env(&MapEnv::from_pairs([
        (ENV_API_URL, "http://127.0.0.1:1"),
        (ENV_STREAM_URL, origin),
        (
            ENV_STREAM_CA,
            root.to_str().ok_or("native root is not UTF-8")?,
        ),
        (ENV_RUNNER_TOKEN, "agt_r_fixture"),
    ]))?)
}

fn assert_carrier(logs: &Capture, after: usize, transport: Transport) {
    assert!(logs.events().iter().skip(after).any(|event| {
        event.field("event") == Some("runner_stream_connected")
            && event.field("transport") == Some(transport.as_str())
    }));
}

async fn simultaneous(
    session: &Session,
    daemon: &NativeDaemon,
    blocked: &Blocked,
    logs: &Capture,
) -> TestResult {
    let lease = Uuid7::parse(LEASE_ID)?;
    let cancelled = session
        .lease_cancellation(&lease)
        .ok_or("the native offer has no cancellation owner")?;
    let bytes = large_report()?;
    let mut reporting = Box::pin(session.report(&bytes));
    assert!(poll!(&mut reporting).is_pending());
    tokio::time::timeout(Duration::from_secs(5), blocked.report.cancelled()).await?;
    saturate(session, &lease, blocked, logs).await?;
    blocked.assert_pending();
    assert!(poll!(&mut reporting).is_pending());
    daemon.cancel_after_renewal(&lease);
    let renewed = tokio::time::timeout(
        Duration::from_secs(5),
        session.renew(&lease, &af_wire::report::RenewRequest::default()),
    )
    .await??;
    assert_eq!(renewed.lease_expires_at, healthy().granted_until);
    tokio::time::timeout(Duration::from_secs(5), cancelled.cancelled()).await?;
    blocked.assert_pending();
    assert!(poll!(&mut reporting).is_pending());
    blocked.release.cancel();
    let reported = tokio::time::timeout(Duration::from_secs(5), reporting).await??;
    assert!(reported.ok);
    Ok(())
}

async fn saturate(
    session: &Session,
    lease: &Uuid7,
    blocked: &Blocked,
    logs: &Capture,
) -> TestResult {
    let arguments = serde_json::to_string(&serde_json::json!({
        "input":"a".repeat(ACTIVITY_BYTES)
    }))?;
    let activity = ActivityRequest {
        frames: vec![ActivityFrame::ToolCallStarted(ToolCallStarted {
            name: "shell".into(),
            args_redacted: arguments.into(),
            call_id: "pressure-fixture".into(),
        })],
    };
    let after = logs.events().len();
    session.activity(lease, &activity).await?;
    tokio::time::timeout(Duration::from_secs(5), blocked.activity.cancelled()).await?;
    for _ in 0..1000 {
        session.activity(lease, &activity).await?;
        if logs.events().iter().skip(after).any(|event| {
            event.field("event") == Some("runner_stream_activity_dropped")
                && event.field("reason") == Some("queue_capacity")
        }) {
            return Ok(());
        }
        tokio::task::yield_now().await;
    }
    Err("the blocked native activity writer never saturated its real queue".into())
}

fn large_report() -> TestResult<Vec<u8>> {
    let mut report: Value = serde_json::from_slice(&report_body())?;
    *report
        .get_mut("response_text")
        .ok_or("the report fixture omitted its output")? = Value::String("r".repeat(REPORT_BYTES));
    let bytes = serde_json::to_vec(&report)?;
    assert!(bytes.len() > REPORT_BYTES && bytes.len() < af_wire::protocol::MESSAGE_BYTES_MAX);
    Ok(bytes)
}
