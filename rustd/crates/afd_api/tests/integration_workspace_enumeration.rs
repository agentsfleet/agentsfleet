//! A workspace nobody minted answers exactly as one somebody else owns, with the
//! production resolver reading real rows.
//!
//! The ownership layer renders one refusal for a resolver's `None`. What makes
//! that refusal hide which workspace identifiers are real is the resolver
//! answering `None` for both a foreign workspace and an unminted one, and only
//! the statement over Postgres can say so. Were the two distinguishable, this
//! surface would be an oracle for which identifiers exist, probeable without
//! holding any of them.
#![cfg(feature = "test-util")]

use afd_db::test_util::mint_id;
use http::{Method, StatusCode};
use serde_json::Value;

use crate::harness::exchange;
use crate::integration_workspace_members::fixture::{Members, owner_scopes};

/// One workspace's history: the cheapest ownership-checked read.
fn history(workspace: &str) -> String {
    format!("/v1/workspaces/{workspace}/events")
}

/// The status, registry code and sentence a refusal carries.
fn refusal((status, body): (StatusCode, Value)) -> (StatusCode, Option<String>, Option<String>) {
    let read = |key: &str| body.get(key).and_then(Value::as_str).map(str::to_owned);
    (status, read("error_code"), read("detail"))
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_workspace_that_does_not_exist_is_not_distinguishable_from_a_foreign_one() {
    let members = Members::create().await;
    let stranger = &members.stranger;
    let router = members.router(stranger, owner_scopes());
    let token = Some(stranger.token.as_str());

    let own = exchange(
        &router,
        Method::GET,
        &history(stranger.workspace.as_str()),
        token,
        "",
    )
    .await;
    let foreign = exchange(
        &router,
        Method::GET,
        &history(members.john.workspace.as_str()),
        token,
        "",
    )
    .await;
    let unminted = exchange(&router, Method::GET, &history(&mint_id()), token, "").await;

    assert_eq!(
        own.0,
        StatusCode::OK,
        "the stranger reads their own: {}",
        own.1
    );
    let foreign = refusal(foreign);
    assert_ne!(
        foreign.0,
        StatusCode::OK,
        "John's workspace is not the stranger's"
    );
    assert_eq!(foreign, refusal(unminted));
    members.cleanup().await;
}
