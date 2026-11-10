//! Native observations retain coded refusals, lease identity and terminal settlement evidence.

use super::super::raw::Native;
use super::{PUSH_BOUND, TestResult};
use crate::{e2e::Scenario, wire};
use af_core::{error_code::ErrorCode, id::Uuid7, timing::STREAM_ANSWER_TIMEOUT};
use af_stream::{Connection, Endpoint, Outgoing, Transport};
use af_wire::protocol::{self, StreamKind};
use axum::http;
use serde_json::{Value, json};
use tokio::{io::AsyncReadExt as _, time::Instant};
use tokio_util::sync::CancellationToken;

pub(super) struct Held {
    pub(super) lease: String,
    pub(super) fence: u64,
    pub(super) event: Uuid7,
    pub(super) fleet: Uuid7,
    pub(super) expires: jiff::Timestamp,
}

pub(super) async fn offer(native: &mut Native) -> TestResult<Held> {
    let frame = native.expect(StreamKind::Work, "offer").await;
    let lease = wire::pointer(&frame, "/lease");
    let (id, fence) = wire::claim(lease);
    Ok(Held {
        lease: id,
        fence,
        fleet: Uuid7::parse(
            wire::pointer(lease, "/event/fleet_id")
                .as_str()
                .ok_or("fleet id missing")?,
        )?,
        event: Uuid7::parse(
            wire::pointer(lease, "/event/event_id")
                .as_str()
                .ok_or("event id missing")?,
        )?,
        expires: wire::pointer(lease, "/lease_expires_at")
            .as_str()
            .ok_or("expiry missing")?
            .parse()?,
    })
}

pub(super) async fn renew(native: &mut Native, held: &[&Held], request_id: u64) -> Value {
    let leases: Vec<_> = held
        .iter()
        .map(|lease| json!({"lease_id":lease.lease,"request":{"requests":[]}}))
        .collect();
    native
        .send(
            StreamKind::Control,
            &json!({
                "type":"renew_batch", "request_id":request_id, "leases":leases,
            }),
        )
        .await;
    let renewed = native.expect(StreamKind::Control, "renewed").await;
    assert_eq!(wire::pointer(&renewed, "/request_id"), request_id);
    renewed
}

pub(super) async fn cancelled(native: &mut Native, held: &Held) -> TestResult {
    let cancel =
        tokio::time::timeout(PUSH_BOUND, native.expect(StreamKind::Control, "cancel")).await?;
    assert_eq!(wire::pointer(&cancel, "/lease_id"), &held.lease);
    assert_eq!(
        wire::pointer(&cancel, "/error_code"),
        af_core::error_code::RUN_LEASE_CANCELLED.as_str()
    );
    Ok(())
}

pub(super) async fn away(native: &mut Native, code: ErrorCode) -> TestResult {
    tokio::time::timeout(PUSH_BOUND, async {
        let away = native.expect(StreamKind::Control, "go_away").await;
        assert_eq!(wire::pointer(&away, "/error_code"), code.as_str());
        native.ended(PUSH_BOUND).await;
    })
    .await?;
    Ok(())
}

pub(super) async fn report(
    run: &Scenario,
    native: &mut Native,
    held: &Held,
    cancelled: bool,
) -> TestResult {
    let mut report = wire::report_body(&held.lease, &held.event, held.fence);
    if cancelled {
        let fields = report.as_object_mut().ok_or("report object missing")?;
        fields.insert("outcome".into(), "fleet_error".into());
        fields.insert("failure_reason".into(), "renewal_terminate".into());
        fields.insert("failure_detail".into(), "the daemon ended the lease".into());
    }
    native
        .send(
            StreamKind::Reports,
            &json!({"type":"report","request_id":20,"request":report}),
        )
        .await;
    let ack = native.expect(StreamKind::Reports, "report_ack").await;
    assert_eq!(wire::pointer(&ack, "/answer/status"), "ok");
    assert_settled(run, held, cancelled).await
}

async fn assert_settled(run: &Scenario, held: &Held, cancelled: bool) -> TestResult {
    let saved: (String, Option<String>) = sqlx::query_as(
        "SELECT status,failure_reason FROM fleet.events WHERE event_id=$1 AND fleet_id=$2",
    )
    .bind(&held.event)
    .bind(&held.fleet)
    .fetch_one(&mut *run.booted.database.acquire().await?)
    .await?;
    assert_eq!(
        saved.0,
        if cancelled {
            af_core::event::status::FLEET_ERROR
        } else {
            af_core::event::status::PROCESSED
        }
    );
    assert_eq!(saved.1.as_deref(), cancelled.then_some("renewal_terminate"));
    Ok(())
}

pub(super) async fn no_offer(native: &mut Native) -> TestResult {
    let ended = tokio::time::timeout(PUSH_BOUND, native.next(StreamKind::Work)).await?;
    assert!(
        !matches!(ended, Ok(Some(_))),
        "a terminal owner received an offer"
    );
    Ok(())
}

pub(super) async fn opening_refused(
    run: &Scenario,
    token: &str,
    code: ErrorCode,
    transport: Transport,
) -> TestResult {
    let stop = CancellationToken::new();
    let endpoint = Endpoint::parse(&format!(
        "https://localhost:{}",
        run.booted.runner_stream_address.port()
    ))?;
    let mut connection = Connection::connect(
        transport,
        &endpoint,
        &crate::support::runner_stream::trust(),
        &stop,
        Instant::now() + STREAM_ANSWER_TIMEOUT,
    )
    .await?;
    let request = opening(&endpoint, token)?;
    let response = connection
        .open(request, Instant::now() + STREAM_ANSWER_TIMEOUT)
        .await?;
    assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);
    let mut bytes = Vec::new();
    response
        .into_body()
        .reader()
        .take(16_384)
        .read_to_end(&mut bytes)
        .await?;
    let body: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(wire::pointer(&body, "/error_code"), code.as_str());
    let _peer_already_closed = connection
        .close(Instant::now() + STREAM_ANSWER_TIMEOUT)
        .await;
    Ok(())
}

fn opening(endpoint: &Endpoint, token: &str) -> TestResult<http::Request<Outgoing>> {
    Ok(http::Request::post(endpoint.uri().clone())
        .header(protocol::VERSION_HEADER, protocol::VERSION)
        .header(protocol::STREAM_KIND_HEADER, StreamKind::Control.as_str())
        .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Outgoing::new(axum::body::Body::empty()))?)
}
