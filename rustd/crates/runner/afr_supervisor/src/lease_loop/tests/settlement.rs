#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test fixtures fail loudly"
)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::client::{Call, Verb};
use crate::error;
use crate::test_support::{
    Answer, Behaviour, FLEET_ID, FakeAgent, FakeEngine, LEASE_ID, Rig, daemon, lease, reported,
};
use af_core::error_code;

fn rig(special: impl Fn(&Call) -> Option<Answer> + Send + Sync + 'static) -> Rig {
    Rig::new(
        daemon(special),
        FakeEngine::default(),
        FakeAgent::new(Behaviour::Answer),
    )
}

fn report_bodies(calls: &[Call]) -> Vec<&[u8]> {
    calls
        .iter()
        .filter(|call| call.verb == Verb::Report)
        .map(|call| call.body.as_ref().unwrap().as_ref())
        .collect()
}

#[tokio::test(start_paused = true)]
async fn disconnect_retries_the_same_complete_report_from_memory() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&attempts);
    let mut rig = rig(move |call| {
        (call.verb == Verb::Report && count.fetch_add(1, Ordering::SeqCst) == 0)
            .then(|| Answer::Fail(error::ErrorKind::Disconnected.into()))
    });
    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();
    let calls = rig.calls();
    let reports = report_bodies(&calls);
    assert_eq!(reports.len(), 2);
    assert_eq!(reports[0], reports[1]);
    assert_eq!(reported(&calls)["memory"][0]["key"], "k");
    assert!(
        !rig.home
            .sandboxes()
            .parent()
            .unwrap()
            .join("spool")
            .exists()
    );
}

#[tokio::test(start_paused = true)]
async fn refusing_the_report_format_never_rewrites_or_resends_it() {
    let mut rig = rig(|call| {
        (call.verb == Verb::Report).then(|| {
            Answer::Fail(
                error::ErrorKind::StreamRefused {
                    code: error_code::INVALID_REQUEST,
                    retryable: false,
                }
                .into(),
            )
        })
    });
    let failure = rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap_err();
    assert_eq!(failure.refusal_code(), Some(error_code::INVALID_REQUEST));
    let calls = rig.calls();
    assert_eq!(report_bodies(&calls).len(), 1);
    assert!(calls.iter().all(|call| call.verb != Verb::Capture));
    assert_eq!(reported(&calls)["memory"][0]["content"], "v");
    assert!(
        !rig.home
            .sandboxes()
            .parent()
            .unwrap()
            .join("spool")
            .exists()
    );
}

#[tokio::test(start_paused = true)]
async fn missing_inline_memory_does_not_change_the_report_format() {
    let mut rig = rig(|_call| None);
    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();
    let calls = rig.calls();
    assert!(calls.iter().all(|call| call.verb != Verb::Capture));
    let body = reported(&calls);
    assert_eq!(body["memory"][0]["key"], "k");
    assert!(body["telemetry"]["segments"]["settle_ms"].is_u64());
}

#[tokio::test(start_paused = true)]
async fn a_report_refused_for_the_token_stops_the_runner_without_a_disk_copy() {
    let rig = rig(|call| {
        (call.verb == Verb::Report).then(|| {
            Answer::Fail(
                error::ErrorKind::StreamRefused {
                    code: error_code::RUN_INVALID_RUNNER_TOKEN,
                    retryable: false,
                }
                .into(),
            )
        })
    });
    assert!(
        rig.run(&lease(LEASE_ID, FLEET_ID))
            .await
            .unwrap_err()
            .is_unauthorized()
    );
    assert!(rig.lessee.halt.token_refused());
    assert!(
        !rig.home
            .sandboxes()
            .parent()
            .unwrap()
            .join("spool")
            .exists()
    );
}

#[tokio::test(start_paused = true)]
async fn a_memory_window_is_not_used_as_a_daemon_version_signal() {
    let bodies = Arc::new(Mutex::new(Vec::new()));
    for inline in [false, true] {
        let saved = Arc::clone(&bodies);
        let rig = rig(move |call| {
            if call.verb == Verb::Report {
                let body: serde_json::Value =
                    serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
                saved.lock().unwrap().push(body);
            }
            None
        });
        let mut payload = lease(LEASE_ID, FLEET_ID);
        if inline {
            payload.memory = Some(af_wire::memory::MemoryHydrateResponse {
                memory: Vec::new(),
                shared: Vec::new(),
                publish: false,
            });
        }
        rig.run(&payload).await.unwrap();
    }
    let bodies = bodies.lock().unwrap();
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[0]["memory"], bodies[1]["memory"]);
    assert!(
        bodies
            .iter()
            .all(|body| body["telemetry"].get("segments").is_some())
    );
}
