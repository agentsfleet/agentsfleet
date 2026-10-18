//! Dimension 4.7: the published field names match the columns they read, and
//! every instant they carry is an RFC 3339 string.
#![expect(
    clippy::unwrap_used,
    reason = "test target: a body that will not decode is an unmet precondition"
)]

use std::borrow::Cow;

use afd_api_wire::admin::{AdminState, RunnerAdminPatchResponse};
use afd_api_wire::approval::ApprovalSummary;
use afd_api_wire::ingress::PullRequestDigest;
use afd_api_wire::operator::RunnerItem;
use serde_json::{Value, json};

/// A whole-second instant, as jiff writes it.
const RAISED: &str = "2026-10-18T06:10:00Z";
/// A millisecond instant, as jiff writes it.
const EXPIRES: &str = "2026-10-18T06:40:00.25Z";

/// One runner list row as the daemon sends it, `status` included.
fn runner_item() -> Value {
    json!({
        "id": "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010",
        "host_id": "host-1",
        "sandbox_tier": "dev_none",
        "status": "cordoned",
        "liveness": "online",
        "labels": [],
        "last_seen_at": RAISED,
        "created_at": RAISED,
        "assigned_policy": {
            "sandbox_tier": "dev_none", "network_policy": "allow_all",
            "registry_allowlist": [], "worker_count": 1, "extra_binds": []
        },
        "achievable": null,
        "degraded": false,
        "degraded_reason": null
    })
}

/// `body` with `from` renamed to `to`.
fn renamed(mut body: Value, from: &str, to: &str) -> Value {
    let object = body.as_object_mut().unwrap();
    let value = object.remove(from).unwrap();
    object.insert(to.to_owned(), value);
    body
}

#[test]
fn test_a_runner_row_names_its_operator_state_status() {
    let body = runner_item().to_string();
    let item: RunnerItem<'_> = serde_json::from_str(&body).unwrap();
    assert_eq!(item.status, AdminState::Cordoned);

    let old = renamed(runner_item(), "status", "admin_state").to_string();
    assert!(
        serde_json::from_str::<RunnerItem<'_>>(&old).is_err(),
        "{old}"
    );
}

#[test]
fn test_a_runner_patch_reply_names_its_operator_state_status() {
    let reply = RunnerAdminPatchResponse {
        id: Cow::Borrowed("0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010"),
        status: AdminState::Draining,
        assigned_policy: None,
        labels: None,
        selftest_requested_at: None,
    };

    let written = serde_json::to_value(&reply).unwrap();

    assert_eq!(written.get("status"), Some(&json!("draining")));
    assert!(written.get("admin_state").is_none(), "{written}");
}

#[test]
fn test_an_approval_names_its_gate_type_and_its_instants_by_column() {
    let summary = ApprovalSummary {
        gate_id: Cow::Borrowed("gate"),
        fleet_id: Cow::Borrowed("fleet"),
        fleet_name: Cow::Borrowed("Production"),
        workspace_id: Cow::Borrowed("workspace"),
        action_id: Cow::Borrowed("action"),
        tool_name: Cow::Borrowed("github"),
        action_name: Cow::Borrowed("merge"),
        gate_type: Cow::Borrowed("approval"),
        proposed_action: Cow::Borrowed("merge the pull request"),
        blast_radius: Cow::Borrowed("repository"),
        status: Cow::Borrowed("approved"),
        detail: Cow::Borrowed(""),
        created_at: RAISED.parse().unwrap(),
        expires_at: EXPIRES.parse().unwrap(),
        resolved_at: Some(RAISED.parse().unwrap()),
        resolved_by: Cow::Borrowed("user"),
        resolved_by_name: Cow::Borrowed(""),
        evidence: None,
    };

    let written = serde_json::to_value(&summary).unwrap();

    assert_eq!(written.get("gate_type"), Some(&json!("approval")));
    assert_eq!(written.get("created_at"), Some(&json!(RAISED)));
    assert_eq!(written.get("expires_at"), Some(&json!(EXPIRES)));
    assert_eq!(written.get("resolved_at"), Some(&json!(RAISED)));
    for old in ["gate_kind", "timeout_at", "updated_at"] {
        assert!(written.get(old).is_none(), "{old} in {written}");
    }
}

#[test]
fn test_a_digest_stamps_its_receipt_as_an_rfc3339_instant() {
    let digest = PullRequestDigest {
        action: Cow::Borrowed("opened"),
        repo: Cow::Borrowed("acme/widget"),
        number: 7,
        title: Cow::Borrowed("Fix"),
        url: Cow::Borrowed("https://github.com/acme/widget/pull/7"),
        state: Cow::Borrowed("open"),
        draft: false,
        author: Cow::Borrowed("octocat"),
        head_ref: Cow::Borrowed("fix"),
        base_ref: Cow::Borrowed("main"),
        head_sha: Cow::Borrowed("abc123"),
        received_at: RAISED.parse().unwrap(),
    };

    let written = serde_json::to_value(&digest).unwrap();

    assert_eq!(written.get("received_at"), Some(&json!(RAISED)));
}
