//! Native report acknowledgements retain the handler's completion analytics link.

use super::{Fixture, acquire};
use crate::harness::{self, Fleet, native::Session};
use af_crypto::entropy::Entropy;
use af_observability::Telemetry;
use af_wire::protocol::StreamKind;
use afd_admission::Admissions;
use afd_auth::scope::ScopeSet;
use afd_fleet::lease::Leases;
use serde_json::{Value, json};

// Value: protects=acknowledged failed native reports emit their completion outcomes;
// fails_when=the gateway omits observation or loses the fixture's analytics handle;
// why_new=observer-direct tests cannot prove native dispatch invokes the observer;
// seam=production native router and real issued fleet leases over existing live stores.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn failed_native_reports_emit_classified_and_unclassified_fleet_errors() {
    let mut fixture = Fixture::create().await;
    let (services, recorded) = Fleet::live(
        fixture.database.clone(),
        "native_decision_http",
        ScopeSet::EMPTY,
    )
    .with_native_authoring(fixture.database.clone())
    .with_recorded_analytics();
    let router = services.router();
    let mut session = Session::open(&router, Some(&fixture.runner.token))
        .await
        .expect("the enrolled runner opens a native session");
    report(&mut session, &fixture, 2, Some("timeout_kill")).await;
    let bus = harness::bus().await;
    let admissions = Admissions::for_tests(fixture.database.clone(), bus.clone());
    let leases = Leases::new(fixture.database.clone(), bus, Entropy::new());
    fixture.lease = acquire(&leases, &admissions, &fixture.owner, &fixture.runner).await;
    report(&mut session, &fixture, 3, None).await;
    let exits: Vec<_> = recorded
        .events()
        .into_iter()
        .filter_map(|event| match event {
            Telemetry::FleetCompleted { exit_status, .. } => Some(exit_status),
            _ => None,
        })
        .collect();
    assert_eq!(exits, ["fleet_error", "fleet_error"]);
    drop(session);
    drop(router);
    fixture.finish().await;
}

async fn report(session: &mut Session, fixture: &Fixture, request_id: u64, reason: Option<&str>) {
    let lease = &fixture.lease;
    session
        .write(
            StreamKind::Reports,
            &json!({"type":"report","request_id":request_id,"request":{
                "lease_id":lease.id,"event_id":lease.source,"fencing_token":lease.fence,
                "outcome":"fleet_error","failure_reason":reason,"failure_detail":"",
                "response_text":"","tokens":0,"requests":[],
                "telemetry":{"time_to_first_token_ms":0,"wall_ms":1},
                "checkpoint":{"last_event_id":lease.source,"last_response":""}
            }}),
        )
        .await;
    let frame = session
        .read(StreamKind::Reports)
        .await
        .expect("the failed run receives its native acknowledgement");
    let reply: Value = frame.decode().expect("native report acknowledgement JSON");
    assert_eq!(
        reply,
        json!({"type":"report_ack","request_id":request_id,"lease_id":lease.id,
            "answer":{"status":"ok","response":{"ok":true}}})
    );
}
