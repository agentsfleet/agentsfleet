//! `M253_003` §2, Dimension 2.5: a member of another workspace cannot answer a
//! gate, whatever its deadline.
#![cfg(feature = "test-util")]

use crate::harness;
use crate::integration_workspace_approvals_fixture::Fixture;

use af_core::error_code;
use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, json_body, send};

/// The workspace whose gate is answered.
const OWNER_SUBJECT: &str = "user_live_approval_owner";

/// A person signed in to another workspace.
const FOREIGN_SUBJECT: &str = "user_live_approval_foreign";

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_member_of_another_workspace_cannot_answer_a_gate() {
    let owner = Fixture::create_as(OWNER_SUBJECT).await;
    owner.seed().await;
    let foreign = Fixture::create_as(FOREIGN_SUBJECT).await;
    foreign.seed().await;
    let router = Fleet::live(
        foreign.database.clone(),
        FOREIGN_SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .with_owned_workspace(foreign.workspace.clone())
    .with_approval_queue(foreign.database.clone(), harness::bus().await)
    .router();

    let decided = send(
        &router,
        Method::POST,
        &format!(
            "/v1/workspaces/{}/approvals/{}/approve",
            foreign.workspace.as_str(),
            owner.gate.as_str()
        ),
        Some(&foreign.token),
        "",
    )
    .await;
    let status = decided.status();
    let decided = json_body(decided).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{decided}");
    assert_eq!(
        decided.get("error_code").and_then(Value::as_str),
        Some(error_code::APPROVAL_NOT_FOUND.as_str())
    );

    foreign.cleanup().await;
    owner.cleanup().await;
}
