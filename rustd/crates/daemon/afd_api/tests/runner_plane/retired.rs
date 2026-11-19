//! Removed runner operations have no HTTP adapter, regardless of credentials or body.

use super::{RUN_ID, RUNNER_TOKEN, TENANT_KEY};
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
        format!("{prefix}/runs"),
        format!("{prefix}/heartbeats"),
        format!("{prefix}/reports"),
        format!("{prefix}/runs/{RUN_ID}/renew"),
        format!("{prefix}/runs/{RUN_ID}/activity"),
        format!("{prefix}/runs/{RUN_ID}/folds"),
        format!("{prefix}/runs/x/renew"),
        format!("{prefix}/authoring-runs/{RUN_ID}/activity"),
        format!("{prefix}/authoring-runs/{RUN_ID}/model-calls"),
        format!("{prefix}/authoring-runs/{RUN_ID}/renew"),
        format!("{prefix}/authoring-runs/{RUN_ID}/reports"),
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
