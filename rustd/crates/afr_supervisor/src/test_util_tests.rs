//! The healthy daemon's routes agree with the client's: a request the client
//! sends for a verb is answered, by path, as that verb is.

#![expect(
    clippy::unwrap_used,
    reason = "test module: a fake daemon that cannot answer is a broken test"
)]

use std::borrow::Cow;

use afd_core::id::Uuid7;
use afd_wire::context::ContextPut;
use afd_wire::memory::{MemoryPushRequest, MemoryRecallRequest};
use afd_wire::report::RenewRequest;
use afd_wire::runner::{AssignedPolicy, HeartbeatRequest, NetworkPolicy, SandboxTier};
use bytes::Bytes;

use super::{FLEET_ID, Healthy, LEASE_ID};
use crate::test_support::{drain, json, plane};

/// A daemon assigning a deny-all posture, beating every half second.
fn healthy() -> Healthy {
    Healthy {
        assigned: AssignedPolicy {
            sandbox_tier: SandboxTier::LandlockFull,
            network_policy: NetworkPolicy::DenyAllEgress,
            registry_allowlist: Vec::new(),
            worker_count: 1,
            extra_binds: Vec::new(),
        },
        interval_ms: 500,
        granted_until: jiff::Timestamp::constant(9, 0),
    }
}

/// Every verb with a reply of its own, and one only acknowledged, sent by the
/// client: each path answers as its verb does.
#[tokio::test]
async fn test_each_route_answers_as_its_verb() {
    let answers = healthy();
    let (plane, mut calls) = plane(move |call| json(&answers.to(Some(call.verb))));
    let (lease, fleet) = (
        Uuid7::parse(LEASE_ID).unwrap(),
        Uuid7::parse(FLEET_ID).unwrap(),
    );
    let beat = HeartbeatRequest {
        capability_report: None,
        selftest: None,
        holds: afd_wire::runner::HeldFleets::default(),
        closing: false,
    };
    let push = MemoryPushRequest {
        lease_id: LEASE_ID.into(),
        fencing_token: 1,
        memory: Vec::new(),
    };
    let recall = MemoryRecallRequest {
        lease_id: LEASE_ID.into(),
        fencing_token: 1,
        query: Cow::Borrowed("q"),
        limit: 1,
    };

    plane.heartbeat(&beat).await.unwrap();
    plane.lease(&[]).await.unwrap();
    plane.renew(&lease, &RenewRequest::default()).await.unwrap();
    plane.hydrate(&fleet).await.unwrap();
    plane.capture(&fleet, &push).await.unwrap();
    plane.recall(&fleet, &recall).await.unwrap();
    plane.me().await.unwrap();
    plane.tool_calls(&lease, Bytes::new()).await.unwrap();
    plane.report(Bytes::new()).await.unwrap();

    let healthy = healthy();
    let sent = drain(&mut calls);
    assert_eq!(sent.len(), 9, "every request reached the daemon");
    for call in sent {
        let method = call.verb.method().http();
        assert_eq!(
            healthy.reply(method.as_str(), &call.path),
            healthy.to(Some(call.verb)),
            "{:?} at {}",
            call.verb,
            call.path
        );
    }
}

#[tokio::test]
async fn context_routes_answer_through_the_native_client() {
    let answers = healthy();
    let (plane, mut calls) =
        plane(move |call| json(&answers.reply(call.verb.method().http().as_str(), &call.path)));
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    assert!(
        plane
            .context_load(&lease, LEASE_ID, 1)
            .await
            .unwrap()
            .checkpoint
            .is_none()
    );
    plane
        .context_commit(
            &lease,
            LEASE_ID,
            &ContextPut {
                fencing_token: 1,
                expected_generation: 0,
                checkpoint: serde_json::json!({}),
            },
        )
        .await
        .unwrap();
    let sent = drain(&mut calls);
    assert_eq!(sent.len(), 2);
    for call in sent {
        assert_eq!(
            healthy().reply(call.verb.method().http().as_str(), &call.path),
            healthy().to(Some(call.verb))
        );
        assert_eq!(
            healthy().reply("POST", &call.path),
            serde_json::json!({"ok":true})
        );
    }
}

// Value: the runtime adapter validates restored notes and binds writes to the lease fence.
#[tokio::test]
async fn lease_context_restores_and_commits_through_the_native_client() {
    use afr_agent::ContextCheckpoints as _;
    let checkpoint: afd_context::Checkpoint = serde_json::from_value(serde_json::json!({
        "format_version":1,
        "identity":{"scope":"private-run","model":"selected-model","pinned":"revision"},
        "generation":1,"covered_through":2,"source_digest":"a".repeat(64),
        "summary":{"goal":"private goal","constraints":[],"decisions":[],
            "completed":[],"remaining":[],"references":[]}
    }))
    .unwrap();
    let document = serde_json::to_value(&checkpoint).unwrap();
    let restored = document.clone();
    let (plane, mut calls) = plane(move |call| {
        if call.verb == crate::client::Verb::ContextLoad {
            json(&serde_json::json!({"checkpoint":restored}))
        } else {
            json(&healthy().reply(call.verb.method().http().as_str(), &call.path))
        }
    });
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let context = crate::context::LeaseContext::new(&plane, &lease, super::FENCING);
    assert_eq!(
        context.load(LEASE_ID).await.unwrap(),
        Some(checkpoint.clone())
    );
    context.commit(LEASE_ID, 0, checkpoint).await.unwrap();
    let sent = drain(&mut calls);
    assert_eq!(sent.len(), 2);
    let write = sent
        .iter()
        .find(|call| call.verb == crate::client::Verb::ContextCommit)
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(write.body.as_ref().unwrap()).unwrap();
    assert_eq!(
        body.get("fencing_token"),
        Some(&serde_json::json!(super::FENCING))
    );
    assert_eq!(body.get("expected_generation"), Some(&serde_json::json!(0)));
    assert_eq!(body.get("checkpoint"), Some(&document));
}
