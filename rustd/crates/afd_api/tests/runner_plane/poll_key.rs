//! The lease poll's optional `Idempotency-Key`, as the route reads it.

use afd_auth::directory::Liveness;
use http::{Method, StatusCode};

use super::satellites::ACCEPTED_KINDS;
use super::{RUNNER_TOKEN, code_of};
use crate::harness::{Fleet, runner_id, send_with_headers};

/// A poll's `Idempotency-Key` is optional, and one that is not a single
/// `UUIDv7` is the invalid-request problem before placement runs; a
/// well-formed key polls as any poll does.
#[tokio::test]
async fn runner_lease_poll_refuses_a_malformed_idempotency_key() {
    let fleet = Fleet::new().with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live);
    let plane = fleet.lease_plane();
    let router = fleet.router();
    let well_formed = "0195b4ba-8d3a-7000-8abc-000000000001";
    let body = format!("{{{ACCEPTED_KINDS}}}");
    let key = http::HeaderName::from_static("idempotency-key");

    let refused = send_with_headers(
        &router,
        Method::POST,
        afd_wire::paths::RUNNER_LEASES,
        Some(RUNNER_TOKEN),
        &body,
        &[(key.clone(), "not-a-uuid")],
    )
    .await;
    let status = refused.status();
    let code = code_of(refused).await;
    let polled = send_with_headers(
        &router,
        Method::POST,
        afd_wire::paths::RUNNER_LEASES,
        Some(RUNNER_TOKEN),
        &body,
        &[(key, well_formed)],
    )
    .await;

    assert_eq!(
        (status, code.as_str(), polled.status()),
        (
            StatusCode::BAD_REQUEST,
            afd_core::error_code::INVALID_REQUEST.as_str(),
            StatusCode::OK,
        )
    );
    assert_eq!(
        plane.polled_keys(),
        vec![Some(well_formed.to_owned())],
        "the refused poll never reached the plane, and the accepted one carried its key"
    );
}
