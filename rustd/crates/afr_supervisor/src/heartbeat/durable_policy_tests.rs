//! Assigned task preference never changes the runner's software-support proof.

#![expect(clippy::expect_used, reason = "test fixture failures must fail loudly")]

use super::{
    Heartbeat,
    tests::{probe, sent},
};
use crate::holds::Holds;
use crate::test_support::{drain, json, plane};
use afd_core::clock::SystemClock;
use std::sync::Arc;

#[tokio::test]
async fn disabling_new_durable_work_keeps_software_capability_and_ordinary_workers() {
    for enabled in [true, false] {
        let (plane, mut calls) = plane(move |_| {
            json(&serde_json::json!({
                "status": "ok", "assigned_policy": {
                    "durable_tasks_enabled": enabled,
                    "sandbox_tier": "landlock_full", "network_policy": "allow_all",
                    "registry_allowlist": [], "worker_count": 1, "extra_binds": [],
                },
                "degraded": false, "degraded_reason": null, "selftest_requested": false,
                "heartbeat_interval_ms": 2000, "release_holds": [],
            }))
        });
        let probe = probe();
        let holds = Holds::start(Arc::new(SystemClock));
        let mut heartbeat = Heartbeat::new(&plane, &probe, &holds);
        let assignment = heartbeat.beat().await.expect("saved assignment decodes");
        assert!(assignment.takes_work(0));
        assert!(!assignment.takes_work(1));
        let calls = drain(&mut calls);
        assert_eq!(calls.len(), 1);
        let body = sent(calls.first().expect("one heartbeat request"));
        assert_eq!(
            body.get("capability_report")
                .and_then(|report| report.get("durable_tasks_v1")),
            Some(&serde_json::json!(true))
        );
    }
}
