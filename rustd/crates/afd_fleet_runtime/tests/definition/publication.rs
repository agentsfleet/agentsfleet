//! Native publication authority survives parsing and narrows for evidence reads.

use super::{parse, value};
use afd_fleet_runtime::Definition;
use afd_fleet_runtime::config::{Access, RepositoryPublication};
use serde_json::{Value, json};

fn native() -> Value {
    let mut input = value();
    let runtime = input.pointer_mut("/config/x-agentsfleet").expect("runtime");
    runtime["repositories"] = json!(["Acme/Checkout", "acme/second"]);
    runtime["repository_publication"] = json!("existing_pr");
    runtime["repository_validation_workflows"] = json!([
        {"path":".github/workflows/check.yml", "sha256":"a".repeat(64)},
        {"path":".github/workflows/test.yaml", "sha256":"b".repeat(64)}
    ]);
    input
}

#[test]
fn evidence_tokens_cannot_inherit_native_publication_write_authority() {
    let definition = parse(&native());
    let binding = definition.runtime().repository_binding().expect("binding");
    assert_eq!(binding.publication(), RepositoryPublication::ExistingPr);
    assert_eq!(binding.validation_workflows().len(), 2);
    assert_eq!(binding.access(), Access::Write);
    let read = binding.read_only();
    assert_eq!(read.access(), Access::Read);
    assert_eq!(read.repositories(), binding.repositories());
    assert!(read.validation_workflows().is_empty());
    assert!(read.reach().base().is_none());
    let exact = binding.read_repository("acme/CHECKOUT").expect("held repo");
    assert_eq!(exact.repositories(), &[Box::<str>::from("Acme/Checkout")]);
    assert_eq!(exact.access(), Access::Read);
    assert!(exact.validation_workflows().is_empty());
    assert!(binding.read_repository("acme/foreign").is_none());
    let recorded = serde_json::to_value(binding.recorded()).expect("record");
    assert_eq!(recorded["publication"], "existing_pr");
    assert_eq!(
        recorded["validation_workflows"]
            .as_array()
            .expect("workflows")
            .len(),
        2
    );
}

#[test]
fn native_publication_refuses_missing_or_ambiguous_workflow_authority() {
    for (field, changed) in [
        ("repository_access", json!("read")),
        ("repository_validation_workflows", json!([])),
        (
            "repository_validation_workflows",
            json!([
                {"path":".github/workflows/check.yml", "sha256":"a".repeat(64)},
                {"path":".github/workflows/check.yml", "sha256":"b".repeat(64)}
            ]),
        ),
        (
            "repository_validation_workflows",
            json!([
                {"path":".github/workflows/../deploy.yml", "sha256":"a".repeat(64)}
            ]),
        ),
        (
            "repository_validation_workflows",
            json!([
                {"path":".github/workflows/check.yml", "sha256":"A".repeat(64)}
            ]),
        ),
        ("repository_publication", json!("event_draft")),
    ] {
        let mut input = native();
        input.pointer_mut("/config/x-agentsfleet").expect("runtime")[field] = changed;
        let failure =
            Definition::parse(&input.to_string()).expect_err("invalid publication authority");
        assert_eq!(
            failure.code(),
            afd_core::error_code::AGENTSFLEET_INVALID_CONFIG,
            "{field}"
        );
    }
}

#[test]
fn goal_wording_changes_do_not_authorize_execution_policy_changes() {
    let input = super::decision_policy::document();
    let original = parse(&input);
    let mut wording = input.clone();
    super::set(&mut wording, "/config/name", json!("renamed-goal"));
    super::set(
        &mut wording,
        "/config/x-agentsfleet/decision_policy/completion/statements/0/criteria/true",
        json!("The requested regression has current passing evidence."),
    );
    let changed = parse(&wording);
    assert!(original.runtime().same_task_authority(changed.runtime()));
    for (path, replacement) in [
        ("/config/x-agentsfleet/repository_base", json!("release")),
        ("/config/x-agentsfleet/repositories", json!(["acme/other"])),
        ("/config/x-agentsfleet/tools", json!(["http_request"])),
        (
            "/config/x-agentsfleet/budget/daily_nanos",
            json!("2000000000"),
        ),
        (
            "/config/x-agentsfleet/network/allow",
            json!(["other.example.com"]),
        ),
    ] {
        let mut widened = wording.clone();
        super::set(&mut widened, path, replacement);
        assert!(
            !original
                .runtime()
                .same_task_authority(parse(&widened).runtime()),
            "accepted authority change at {path}"
        );
    }
}
