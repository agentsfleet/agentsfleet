//! A bound pull request stays inside the fleet's repository binding, and is
//! re-read unchanged before anything is written to it.

#![expect(
    clippy::expect_used,
    reason = "a fixture definition or target the test builds fails the test loudly"
)]

use af_fleet_runtime::config::RepositoryBinding;
use serde_json::json;

use crate::{
    Result,
    durable_task::{
        DurableTaskRefusal,
        github::{
            BoundPr, Workflow,
            double::{INSTALLATION, Origin, Reply},
        },
    },
    error::ErrorKind,
};

const HEAD: &str = "1111111111111111111111111111111111111111";
const DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn target() -> BoundPr {
    BoundPr {
        repository: "example/linkwarden".to_owned(),
        repository_id: 42,
        installation_id: INSTALLATION,
        number: 12,
        branch: "repair".to_owned(),
        base: "main".to_owned(),
        head: HEAD.to_owned(),
        tree: "2222222222222222222222222222222222222222".to_owned(),
        url: "https://github.com/example/linkwarden/pull/12".to_owned(),
        workflows: vec![Workflow {
            id: 100,
            path: ".github/workflows/ci.yml".to_owned(),
            digest: DIGEST.to_owned(),
        }],
    }
}

/// The fleet's approved GitHub scope, binding `repository` for write
/// publication into `base` with the one reviewed workflow.
fn config(repository: &str, base: &str) -> RepositoryBinding {
    let scope = json!({
        "repositories": [repository], "access": "write", "base": base,
        "publication": "existing_pr",
        "validation_workflows": [{"path": ".github/workflows/ci.yml", "sha256": DIGEST}]});
    RepositoryBinding::parse(&scope.to_string()).expect("an approved repository scope")
}

fn refusal<T>(result: Result<T>) -> Option<DurableTaskRefusal> {
    match result.err()?.kind() {
        ErrorKind::DurableTaskRefused { reason } => Some(*reason),
        _ => None,
    }
}

// Value: protects=a target is published only while the fleet still binds its
// repository, base and reviewed workflows; fails_when=any one of those checks is
// dropped from `within`; why_new=the refusal had no case; seam=the target's own
// check against a stored fleet definition; no_claim=the remote re-read.
#[test]
fn a_target_outside_the_fleets_repository_binding_is_refused() {
    let target = target();
    assert!(
        target
            .within(Some(&config("Example/Linkwarden", "main")))
            .is_ok(),
        "the bound repository matches without regard to case"
    );
    let mut moved = target.clone();
    moved.workflows = vec![Workflow {
        digest: "b".repeat(64),
        ..target.workflows.first().expect("one workflow").clone()
    }];
    for (refused, config) in [
        (&target, config("example/other", "main")),
        (&target, config("example/linkwarden", "release")),
        (&moved, config("example/linkwarden", "main")),
    ] {
        assert_eq!(
            refusal(refused.within(Some(&config))),
            Some(DurableTaskRefusal::InvalidTarget)
        );
    }
}

/// The open pull request from `repair` into `main` at `head`.
fn pull(head: &str) -> serde_json::Value {
    json!({
        "number": 12, "state": "open", "merged": false,
        "head": {"sha": head, "ref": "repair",
            "repo": {"id": 42, "full_name": "example/linkwarden", "default_branch": "main"}},
        "base": {"sha": "3333333333333333333333333333333333333333", "ref": "main",
            "repo": {"id": 42, "full_name": "example/linkwarden", "default_branch": "main"}},
    })
}

// Value: protects=nothing is written to a pull request whose branch was
// protected or moved after binding; fails_when=the protected or head comparison
// is dropped from the re-read; why_new=the stale refusal had no case; seam=the
// re-read over a scripted TLS origin, through the production session;
// no_claim=the workflow re-read that follows a matching branch.
#[tokio::test]
async fn a_branch_protected_or_moved_since_binding_is_stale() {
    let moved = "4444444444444444444444444444444444444444";
    for (pull_head, protected, branch_head) in [(HEAD, true, HEAD), (moved, false, moved)] {
        let origin = Origin::start(vec![
            Reply::json(&pull(pull_head)),
            Reply::json(&json!({"protected": protected, "commit": {"sha": branch_head}})),
        ])
        .await;
        let verdict = target().verify(&origin.session(false)).await;
        assert_eq!(refusal(verdict), Some(DurableTaskRefusal::StaleState));
        assert_eq!(
            origin.requests(),
            vec![
                "GET /repos/example/linkwarden/pulls/12".to_owned(),
                "GET /repos/example/linkwarden/branches/repair".to_owned(),
            ],
            "no workflow is read once the branch is stale"
        );
    }
}
