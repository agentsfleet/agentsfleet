//! An exact-head observation: the bound pull request must still be the one
//! recorded, every run must be for its head, and each reviewed workflow
//! reports its latest attempt or pending.

use serde_json::{Value, json};

use super::{Check, Snapshot};
use crate::{
    Result,
    error::ErrorKind,
    task::{
        TaskRefusal,
        github::{
            BoundPr, Workflow,
            double::{INSTALLATION, Origin, Reply, credential},
            http::Session,
        },
    },
};

const HEAD: &str = "1111111111111111111111111111111111111111";
const PULL: &str = "GET /repos/example/linkwarden/pulls/12";
const RUNS: &str = "GET /repos/example/linkwarden/actions/runs?head_sha=1111111111111111111111111111111111111111&per_page=100";

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
        workflows: vec![
            Workflow {
                id: 100,
                path: ".github/workflows/ci.yml".to_owned(),
                digest: "a".repeat(64),
            },
            Workflow {
                id: 200,
                path: ".github/workflows/lint.yml".to_owned(),
                digest: "b".repeat(64),
            },
        ],
    }
}

fn check(workflow_id: u64, status: &str, conclusion: Option<&str>) -> Check {
    Check {
        workflow_id,
        run_id: Some(workflow_id + 1),
        run_number: Some(1),
        run_attempt: Some(1),
        status: status.to_owned(),
        conclusion: conclusion.map(str::to_owned),
    }
}

fn snapshot(open: bool, head: &str, checks: Vec<Check>) -> Snapshot {
    Snapshot {
        open,
        head: head.to_owned(),
        checks,
    }
}

/// The open pull request on `repair` into `main`, with `number` as its number.
fn pull(number: u32) -> Value {
    json!({
        "number": number,
        "state": "open",
        "merged": false,
        "head": {"sha": HEAD, "ref": "repair", "repo": {"id": 42, "full_name": "example/linkwarden"}},
        "base": {"sha": "3333333333333333333333333333333333333333", "ref": "main",
            "repo": {"id": 42, "full_name": "Example/Linkwarden"}},
    })
}

fn run(id: u64, workflow_id: u64, run_number: u64, run_attempt: u32, head: &str) -> Value {
    json!({
        "id": id,
        "workflow_id": workflow_id,
        "run_attempt": run_attempt,
        "run_number": run_number,
        "head_sha": head,
        "status": "completed",
        "conclusion": if run_attempt == 1 { "failure" } else { "success" },
    })
}

fn refusal<T>(result: Result<T>) -> Option<TaskRefusal> {
    match result.err()?.kind() {
        ErrorKind::TaskRefused { reason } => Some(*reason),
        _ => None,
    }
}

#[test]
fn green_needs_every_reviewed_workflow_to_succeed_on_the_open_head() {
    let target = target();
    let success = || {
        vec![
            check(100, "completed", Some("success")),
            check(200, "completed", Some("success")),
        ]
    };

    let passed = snapshot(true, HEAD, success());
    assert!(passed.green(&target));
    assert!(passed.checks_finished(&target));

    for (refused, finished) in [
        (
            snapshot(
                true,
                HEAD,
                vec![
                    check(100, "completed", Some("success")),
                    check(200, "completed", Some("failure")),
                ],
            ),
            true,
        ),
        (
            snapshot(
                true,
                HEAD,
                vec![
                    check(100, "completed", Some("success")),
                    check(200, "in_progress", None),
                ],
            ),
            false,
        ),
        (snapshot(false, HEAD, success()), false),
        (
            snapshot(true, "4444444444444444444444444444444444444444", success()),
            false,
        ),
        (snapshot(true, HEAD, Vec::new()), false),
        (
            snapshot(true, HEAD, vec![check(100, "completed", Some("success"))]),
            false,
        ),
    ] {
        assert!(!refused.green(&target), "{refused:?}");
        assert_eq!(refused.checks_finished(&target), finished, "{refused:?}");
    }
}

#[tokio::test]
async fn a_session_for_another_installation_or_a_publisher_reads_nothing() {
    let origin = Origin::start(Vec::new()).await;
    let other = Session::new(&origin.client, credential(), INSTALLATION + 1, true);

    assert_eq!(
        refusal(other.status(&target()).await),
        Some(TaskRefusal::InvalidTarget)
    );
    assert_eq!(
        refusal(origin.session(false).status(&target()).await),
        Some(TaskRefusal::InvalidTarget)
    );
    assert_eq!(origin.requests(), Vec::<String>::new());
}

#[tokio::test]
async fn a_pull_request_that_is_no_longer_the_bound_one_is_an_invalid_target() {
    let origin = Origin::start(vec![Reply::json(&pull(13))]).await;

    assert_eq!(
        refusal(origin.session(true).status(&target()).await),
        Some(TaskRefusal::InvalidTarget)
    );
    assert_eq!(origin.requests(), [PULL]);
}

#[tokio::test]
async fn an_incomplete_run_listing_is_an_invalid_target() {
    let origin = Origin::start(vec![
        Reply::json(&pull(12)),
        Reply::json(&json!({"total_count": 2, "workflow_runs": [run(7, 100, 1, 1, HEAD)]})),
    ])
    .await;

    assert_eq!(
        refusal(origin.session(true).status(&target()).await),
        Some(TaskRefusal::InvalidTarget)
    );
    assert_eq!(origin.requests(), [PULL, RUNS]);
}

#[tokio::test]
async fn a_run_for_another_head_is_an_invalid_target() {
    let origin = Origin::start(vec![
        Reply::json(&pull(12)),
        Reply::json(&json!({"total_count": 1, "workflow_runs": [
            run(7, 100, 1, 1, "5555555555555555555555555555555555555555"),
        ]})),
    ])
    .await;

    assert_eq!(
        refusal(origin.session(true).status(&target()).await),
        Some(TaskRefusal::InvalidTarget)
    );
    assert_eq!(origin.requests(), [PULL, RUNS]);
}

/// Workflow 100 ran twice; the second attempt is the latest. Workflow 200 has
/// not run, so it is pending.
#[tokio::test]
async fn each_reviewed_workflow_reports_its_latest_attempt_or_pending() {
    let origin = Origin::start(vec![
        Reply::json(&pull(12)),
        Reply::json(&json!({"total_count": 2, "workflow_runs": [
            run(7, 100, 5, 1, HEAD),
            run(8, 100, 5, 2, HEAD),
        ]})),
    ])
    .await;

    let observed = origin.session(true).status(&target()).await.ok();

    assert_eq!(
        observed,
        Some(Snapshot {
            open: true,
            head: HEAD.to_owned(),
            checks: vec![
                Check {
                    workflow_id: 100,
                    run_id: Some(8),
                    run_number: Some(5),
                    run_attempt: Some(2),
                    status: "completed".to_owned(),
                    conclusion: Some("success".to_owned()),
                },
                Check {
                    workflow_id: 200,
                    run_id: None,
                    run_number: None,
                    run_attempt: None,
                    status: "pending".to_owned(),
                    conclusion: None,
                },
            ],
        })
    );
    assert_eq!(origin.requests(), [PULL, RUNS]);
}
