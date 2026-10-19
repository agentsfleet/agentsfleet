//! The payload reads, against deliveries GitHub sends and the edges between
//! them: which fields a decision reads, which shapes refuse, and which read.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use super::{Body, Delivery, RunAction};

/// A failed run as an App sends it: the installation rides in the envelope.
const RUN_FAILURE_APP: &str =
    include_str!("../../../../../../../../tests/fixtures/webhooks/github_run_failure_app.json");

/// A pull-request delivery as github.com sends one, its action a placeholder.
const PULL_REQUEST_DELIVERY: &str =
    include_str!("../../../../../../../../tests/fixtures/webhooks/github_pull_request.json");

/// The installation [`RUN_FAILURE_APP`] was sent for.
const FIXTURE_INSTALLATION: u64 = 48_765_123;

/// The repository every fixture is about.
const FIXTURE_REPOSITORY: &str = "example/platform";

#[test]
fn test_github_workflow_run_decodes_without_octocrab() {
    let delivery =
        Delivery::read("workflow_run", RUN_FAILURE_APP.as_bytes()).expect("a delivery GitHub sent");

    assert_eq!(delivery.installation(), Some(FIXTURE_INSTALLATION));
    assert_eq!(delivery.repository(), Some(FIXTURE_REPOSITORY));
    let Body::WorkflowRun(event) = &delivery.body else {
        panic!("a workflow_run header reads a workflow_run body: {delivery:?}");
    };
    assert_eq!(event.action, RunAction::Completed);
    let run = &event.workflow_run;
    assert_eq!(run.conclusion, "failure");
    assert_eq!(run.head_branch, "main");
    assert!(!run.html_url.is_empty(), "{run:?}");
    assert!(!run.head_sha.is_empty(), "{run:?}");
    assert!(!run.name.is_empty(), "{run:?}");
    assert!(run.id > 0, "{run:?}");
    assert!(run.run_attempt >= 1, "{run:?}");
}

#[test]
fn test_github_pull_request_decodes_without_octocrab() {
    let body = PULL_REQUEST_DELIVERY.replace("__ACTION__", "opened");

    let delivery = Delivery::read("pull_request", body.as_bytes()).expect("a delivery GitHub sent");

    // A manual hook's delivery: a repository, and no installation.
    assert_eq!(delivery.installation(), None);
    assert_eq!(delivery.repository(), Some(FIXTURE_REPOSITORY));
    let Body::PullRequest(event) = &delivery.body else {
        panic!("a pull_request header reads a pull_request body: {delivery:?}");
    };
    assert_eq!(event.action, "opened");
    assert_eq!(event.number, 7);
    let pull = &event.pull_request;
    assert_eq!(pull.head.reference, "feature");
    assert_eq!(pull.base.reference, "main");
    assert!(!pull.head.sha.is_empty(), "{pull:?}");
    assert_eq!(pull.state.as_deref(), Some("open"));
    assert_eq!(pull.draft, Some(false));
    assert_eq!(
        pull.user.as_ref().map(|user| user.login.as_str()),
        Some("octocat")
    );
    assert!(pull.title.is_some(), "{pull:?}");
    assert!(pull.html_url.is_some(), "{pull:?}");
}

#[test]
fn a_pull_request_action_github_adds_later_still_reads() {
    // The action is GitHub's word rather than a closed list, so a new one is a
    // delivery the App route can fan out rather than a 4xx.
    let body = PULL_REQUEST_DELIVERY.replace("__ACTION__", "stacked_into_the_future");

    let delivery = Delivery::read("pull_request", body.as_bytes()).expect("a new action reads");

    let Body::PullRequest(event) = &delivery.body else {
        panic!("a pull_request body: {delivery:?}");
    };
    assert_eq!(event.action, "stacked_into_the_future");
}

#[test]
fn a_workflow_run_action_outside_the_three_refuses_the_read() {
    let body = RUN_FAILURE_APP.replacen("\"completed\"", "\"exploded\"", 1);

    Delivery::read("workflow_run", body.as_bytes()).expect_err("an action outside the three");
}

#[test]
fn a_workflow_run_delivery_without_its_run_refuses_the_read() {
    let body = r#"{"action":"completed","repository":{"full_name":"example/platform"}}"#;

    Delivery::read("workflow_run", body.as_bytes()).expect_err("a run delivery names its run");
}

#[test]
fn a_run_whose_fields_are_mistyped_reads_as_an_empty_run() {
    // Present but unreadable: dropped downstream on its empty conclusion,
    // never refused, because GitHub signed it and would not change it.
    let body = r#"{"action":"completed","repository":{"full_name":"example/platform"},
        "workflow_run":{"conclusion":"failure","head_branch":null}}"#;

    let delivery = Delivery::read("workflow_run", body.as_bytes()).expect("the envelope reads");

    let Body::WorkflowRun(event) = &delivery.body else {
        panic!("a workflow_run body: {delivery:?}");
    };
    assert_eq!(event.workflow_run.conclusion, "");
    assert_eq!(event.workflow_run.head_branch, "");
}

#[test]
fn an_event_with_no_rule_reads_only_its_envelope() {
    // `deployment_status` carries a body no reader declares; only the routing
    // fields are read, so its shape cannot refuse it.
    let body = r#"{"installation":{"id":9},"repository":{"full_name":"example/platform"},
        "deployment_status":"not an object at all"}"#;

    let delivery =
        Delivery::read("deployment_status", body.as_bytes()).expect("the envelope reads");

    assert_eq!(delivery.installation(), Some(9));
    assert!(matches!(delivery.body, Body::Other));
}

#[test]
fn a_body_that_is_not_an_object_refuses_the_read() {
    Delivery::read("ping", b"\"pong\"").expect_err("a delivery is a JSON object");
}
