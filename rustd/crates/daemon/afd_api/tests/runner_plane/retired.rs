//! Removed runner operations have no HTTP adapter, regardless of credentials or body.

use super::{LEASE_ID, RUNNER_TOKEN, TENANT_KEY};
use crate::harness::{Fleet, runner_id, send};
use afd_auth::directory::Liveness;
use http::{Method, StatusCode};

#[tokio::test]
async fn retired_runner_routes_never_reach_authentication_or_a_handler() {
    let fleet = Fleet::new().with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live);
    let directory = fleet.directory().clone();
    let router = fleet.router();
    let prefix = af_wire::paths::RUNNER_SELF;
    let paths = [
        format!("{prefix}/leases"),
        format!("{prefix}/heartbeats"),
        format!("{prefix}/reports"),
        format!("{prefix}/leases/{LEASE_ID}/renew"),
        format!("{prefix}/leases/{LEASE_ID}/activity"),
        format!("{prefix}/leases/{LEASE_ID}/folds"),
        format!("{prefix}/leases/x/renew"),
        format!("{prefix}/authoring-leases/{LEASE_ID}/activity"),
        format!("{prefix}/authoring-leases/{LEASE_ID}/model-calls"),
        format!("{prefix}/authoring-leases/{LEASE_ID}/renew"),
        format!("{prefix}/authoring-leases/{LEASE_ID}/reports"),
    ];
    for path in paths {
        for credential in [None, Some(RUNNER_TOKEN), Some(TENANT_KEY)] {
            for body in ["", "not-json", "{}"] {
                let response = send(&router, Method::POST, &path, credential, body).await;
                assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}: {body}");
            }
        }
    }
    assert_eq!(
        directory.lookups(),
        0,
        "an absent route never checks a runner"
    );
}
