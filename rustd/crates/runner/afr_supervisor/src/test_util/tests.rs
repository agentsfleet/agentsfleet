//! The healthy daemon's routes agree with the client's: a request the client
//! sends for a verb is answered, by path, as that verb is.

#![expect(
    clippy::unwrap_used,
    reason = "test module: a fake daemon that cannot answer is a broken test"
)]

use std::borrow::Cow;

use af_core::id::Uuid7;
use af_wire::memory::{MemoryPushRequest, MemoryRecallRequest};
use af_wire::report::RenewRequest;
use af_wire::runner::{AssignedPolicy, HeartbeatRequest, NetworkPolicy, SandboxTier};
use bytes::Bytes;

use super::{FLEET_ID, Healthy, LEASE_ID};
use crate::test_support::{drain, json, plane};

/// A daemon assigning a deny-all posture, beating every half second.
fn healthy() -> Healthy {
    Healthy {
        assigned: AssignedPolicy {
            durable_tasks_enabled: af_wire::runner::DURABLE_TASKS_DEFAULT_ENABLED,
            decision_model_id: None,
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
        holds: af_wire::runner::HeldFleets::default(),
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
    plane.renew(&lease, &RenewRequest::default()).await.unwrap();
    plane.hydrate(&fleet).await.unwrap();
    plane.capture(&fleet, &push).await.unwrap();
    plane.recall(&fleet, &recall).await.unwrap();
    plane.me().await.unwrap();
    plane.tool_calls(&lease, Bytes::new()).await.unwrap();
    plane
        .report(&crate::test_support::report_body())
        .await
        .unwrap();
    plane.folds(&lease, 1, 0).await.unwrap();

    assert_routes(&drain(&mut calls));
}

/// The shared test issuer authenticates the same hybrid-only native transports as the runner.
#[tokio::test]
async fn the_native_fixture_authenticates_both_transports()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut daemon = super::NativeDaemon::serve(
        healthy(),
        serde_json::from_str(super::LEASE_JSON)?,
        |_frame| async {},
    )
    .await?;
    let endpoint = af_stream::Endpoint::parse(&daemon.url)?;
    let trust = af_stream::tls::Trust::with_roots(daemon.root());
    let stop = tokio_util::sync::CancellationToken::new();
    for transport in [af_stream::Transport::H3, af_stream::Transport::H2] {
        let connection = af_stream::Connection::connect(
            transport,
            &endpoint,
            &trust,
            &stop,
            tokio::time::Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT,
        )
        .await?;
        assert_eq!(
            connection.key_exchange(),
            Some(af_stream::tls::KeyExchange::X25519MlKem768)
        );
        connection
            .close(tokio::time::Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT)
            .await?;
    }
    daemon.close().await?;
    Ok(())
}

fn assert_routes(sent: &[crate::client::Call]) {
    let healthy = healthy();
    assert_eq!(sent.len(), 9, "every request reached the daemon");
    for call in sent {
        let method = call.verb.method().http();
        assert_eq!(
            if call.path == af_wire::paths::RUNNER_STREAM {
                healthy.to(Some(call.verb))
            } else {
                healthy.reply(method.as_str(), &call.path)
            },
            healthy.to(Some(call.verb)),
            "{:?} at {}",
            call.verb,
            call.path
        );
    }
}
