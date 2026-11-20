//! Every method uses the daemon's canonical resource path.
use super::Verb;
use crate::test_support::{Answer, FLEET_ID, RUN_ID, drain, json, plane};
use af_core::id::Uuid7;
use af_wire::{
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
            run_expires_at: RENEWED_UNTIL,
            recorded_through: None,
        }),
        _other => json(&crate::test_support::healthy().to(Some(call.verb))),
    });
    assert!(format!("{plane:?}").contains("FakeApi"));

    send_every_verb(&plane).await;

    let mut routes: Vec<_> = drain(&mut calls)
        .into_iter()
        .map(|call| (call.verb, call.path.into_owned(), call.body.is_some()))
        .collect();
    let run_root = format!("/v1/runners/me/runs/{RUN_ID}");
    let memory = format!("/v1/runners/me/memory/{FLEET_ID}");
    let mut expected = vec![
        (Verb::Heartbeat, "/v1/runners/me/stream".to_owned(), true),
        (Verb::Renew, "/v1/runners/me/stream".to_owned(), true),
        (Verb::Activity, "/v1/runners/me/stream".to_owned(), true),
        (Verb::Report, "/v1/runners/me/stream".to_owned(), true),
        (Verb::Load, memory.clone(), false),
        (Verb::Capture, memory, true),
        (
            Verb::Mint,
            "/v1/runners/me/credentials/mint".to_owned(),
            true,
        ),
        (Verb::Items, format!("{run_root}/items"), true),
    ];
    routes.sort_by_key(|route| format!("{:?}", route.0));
    expected.sort_by_key(|route| format!("{:?}", route.0));
    assert_eq!(routes, expected);
}

/// A body whose content no fake daemon reads.
const EMPTY_BODY: &[u8] = b"{}";

/// Sends each verb once, in the order the routes assertion lists them.
async fn send_every_verb(plane: &super::super::ControlPlane) {
    let run = Uuid7::parse(RUN_ID).unwrap();
    let fleet = Uuid7::parse(FLEET_ID).unwrap();
    let heartbeat = HeartbeatRequest {
        capability_report: None,
        selftest: None,
        holds: af_wire::runner::HeldFleets::default(),
        closing: false,
    };
    let push = MemoryPushRequest {
        run_id: RUN_ID.into(),
        fencing_token: 1,
        memory: Vec::new(),
    };
    plane.heartbeat(&heartbeat).await.unwrap();
    assert_eq!(
        plane
            .renew(&run, &RenewRequest::default())
            .await
            .unwrap()
            .run_expires_at,
        RENEWED_UNTIL,
        "renewal returns its new expiry"
    );
    plane
        .activity(&run, &ActivityRequest { frames: Vec::new() })
        .await
        .unwrap();
    plane
        .report(&crate::test_support::report_body())
        .await
        .unwrap();
    plane.load(&fleet).await.unwrap();
    plane.capture(&fleet, &push).await.unwrap();
    let mint = MintCredentialRequest {
        run_id: RUN_ID.into(),
        integration: "github".into(),
        scope: None,
    };
    plane.mint(&mint).await.unwrap();
    plane
        .tool_calls(&run, Bytes::from_static(EMPTY_BODY))
        .await
        .unwrap();
}

#[tokio::test]
async fn a_reply_decodes_borrowing_or_refuses_as_malformed() {
    let (plane, _calls) = plane(|call| match call.verb {
        Verb::Renew => json(&RenewResponse {
            run_expires_at: RENEWED_UNTIL,
            recorded_through: None,
        }),
        _other => Answer::Reply(Bytes::from_static(b"[")),
    });

    let good = plane
        .renew(&Uuid7::parse(RUN_ID).unwrap(), &RenewRequest::default())
        .await
        .unwrap();
    let bad = plane.load(&Uuid7::parse(FLEET_ID).unwrap()).await.unwrap();

    assert_eq!(good.run_expires_at, RENEWED_UNTIL);
    let refused = bad.decode::<RenewResponse>().unwrap_err();
    assert!(
        refused.to_string().contains("load reply did not decode"),
        "{refused}"
    );
}
