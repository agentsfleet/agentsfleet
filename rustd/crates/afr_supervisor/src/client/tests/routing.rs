//! Every method uses the daemon's canonical resource path.
use super::Verb;
use crate::test_support::{Answer, FLEET_ID, LEASE_ID, drain, json, plane};
use afd_core::id::Uuid7;
use afd_wire::{
    activity::ActivityRequest,
    credentials::MintCredentialRequest,
    memory::MemoryPushRequest,
    report::{RenewRequest, RenewResponse},
    runner::HeartbeatRequest,
};
use bytes::Bytes;

/// The expiry every fake renewal grants.
const RENEWED_UNTIL: jiff::Timestamp = jiff::Timestamp::constant(7, 0);

#[tokio::test]
async fn every_verb_goes_to_its_own_path() {
    let (plane, mut calls) = plane(|call| match call.verb {
        Verb::Renew => json(&RenewResponse {
            lease_expires_at: RENEWED_UNTIL,
            recorded_through: None,
        }),
        _other => json(&serde_json::json!({})),
    });
    assert!(format!("{plane:?}").contains("FakeApi"));

    send_every_verb(&plane).await;

    let routes: Vec<_> = drain(&mut calls)
        .into_iter()
        .map(|call| (call.verb, call.path.into_owned(), call.body.is_some()))
        .collect();
    let lease_root = format!("/v1/runners/me/leases/{LEASE_ID}");
    let memory = format!("/v1/runners/me/memory/{FLEET_ID}");
    assert_eq!(
        routes,
        vec![
            (
                Verb::Heartbeat,
                "/v1/runners/me/heartbeats".to_owned(),
                true
            ),
            (Verb::Lease, "/v1/runners/me/leases".to_owned(), true),
            (Verb::Renew, format!("{lease_root}/renew"), true),
            (Verb::Activity, format!("{lease_root}/activity"), true),
            (Verb::Report, "/v1/runners/me/reports".to_owned(), true),
            (Verb::Hydrate, memory.clone(), false),
            (Verb::Capture, memory, true),
            (
                Verb::Mint,
                "/v1/runners/me/credentials/mint".to_owned(),
                true
            ),
            (Verb::Records, format!("{lease_root}/tool-calls"), true),
        ]
    );
}

/// A body whose content no fake daemon reads.
const EMPTY_BODY: &[u8] = b"{}";

/// Sends each verb once, in the order the routes assertion lists them.
async fn send_every_verb(plane: &super::super::ControlPlane) {
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let fleet = Uuid7::parse(FLEET_ID).unwrap();
    let heartbeat = HeartbeatRequest {
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
    plane.heartbeat(&heartbeat).await.unwrap();
    plane.lease(&[]).await.unwrap();
    assert_eq!(
        plane
            .renew(&lease, &RenewRequest::default())
            .await
            .unwrap()
            .lease_expires_at,
        RENEWED_UNTIL,
        "renewal returns its new expiry"
    );
    plane
        .activity(&lease, &ActivityRequest { frames: Vec::new() })
        .await
        .unwrap();
    plane.report(Bytes::from_static(EMPTY_BODY)).await.unwrap();
    plane.hydrate(&fleet).await.unwrap();
    plane.capture(&fleet, &push).await.unwrap();
    let mint = MintCredentialRequest {
        lease_id: LEASE_ID.into(),
        integration: "github".into(),
        scope: None,
    };
    plane.mint(&mint).await.unwrap();
    plane
        .tool_calls(&lease, Bytes::from_static(EMPTY_BODY))
        .await
        .unwrap();
}

#[tokio::test]
async fn a_reply_decodes_borrowing_or_refuses_as_malformed() {
    let (plane, _calls) = plane(|call| match call.verb {
        Verb::Lease => json(&RenewResponse {
            lease_expires_at: RENEWED_UNTIL,
            recorded_through: None,
        }),
        _other => Answer::Reply(Bytes::from_static(b"[")),
    });

    let good = plane.lease(&[]).await.unwrap();
    let bad = plane
        .hydrate(&Uuid7::parse(FLEET_ID).unwrap())
        .await
        .unwrap();

    assert_eq!(
        good.decode::<RenewResponse>().unwrap().lease_expires_at,
        RENEWED_UNTIL
    );
    let refused = bad.decode::<RenewResponse>().unwrap_err();
    assert!(
        refused.to_string().contains("hydrate reply did not decode"),
        "{refused}"
    );
}
