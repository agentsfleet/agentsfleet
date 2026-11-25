//! A running fleet adds, lists and removes what may wake it, through the run
//! it runs under: `POST`, `GET` and `DELETE L/triggers`.
//!
//! Booted as the schedules suites are (`crate::schedules::Assigned`): the
//! daemon's own router and run plane over the lane's Postgres and NATS, the
//! seeded event's run taken. The fleet is always the run's: no body names one.
//! An add carries its fence and its call's item in the body; a read and a
//! removal carry the fence in the query.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly, and a step \
              indexes the JSON it was answered"
)]

use af_crypto::entropy::Entropy;
use afd_connector::Provider;
use afd_db::test_util::InstallSeed;
use agentsfleetd::supervisor::Supervisor;
use reqwest::Method;
use serde_json::{Value, json};

use crate::definition_install::seal;
use crate::schedules::{Assigned, itemized};
use crate::verbs::FakeQStash;

/// The vault entry a fixture webhook is signed with.
const SIGNING_SECRET: &str = "deploys-hook";

/// An identifier no fleet's trigger and no run carries.
const UNKNOWN: &str = "0195b4ba-8d3a-7fff-8abc-ffffffffffff";

/// `tail` under the held run's triggers collection.
fn triggers(held: &Assigned, tail: &str) -> String {
    format!("/v1/runners/me/runs/{}/triggers{tail}", held.run_id)
}

/// [`triggers`], fenced at `fence` in the query, as a read and a removal
/// carry it.
fn fenced(held: &Assigned, tail: &str, fence: u64) -> String {
    format!("{}?fencing_token={fence}", triggers(held, tail))
}

/// A fresh call's webhook from `source`, signed by [`SIGNING_SECRET`] under a
/// body recipe whose header it names, waking on one event.
fn webhook(held: &Assigned, source: &str) -> Value {
    itemized(json!({
        "fencing_token": held.fence,
        "kind": "webhook",
        "source": source,
        "events": ["deploy.failed"],
        "verifier": {"scheme": "body-hex-sha256", "header": "x-deploy-signature"},
        "secret_name": SIGNING_SECRET,
    }))
}

/// A fresh call's GitHub App subscription for `org/app`'s pull requests.
fn app_event(held: &Assigned) -> Value {
    itemized(json!({
        "fencing_token": held.fence,
        "kind": "app_event",
        "events": ["pull_request"],
        "repositories": ["org/app"],
    }))
}

/// Seals the signing secret in the held fleet's workspace.
async fn store_signing_secret(held: &Assigned) {
    let body = json!({"secret": "whsec_fixture"}).to_string();
    let scene = &held.scene;
    seal(
        &scene.booted,
        &scene.workspace,
        SIGNING_SECRET,
        &body,
        af_core::clock::now(),
    )
    .await;
}

/// Routes a GitHub App installation to the held fleet's workspace, as the
/// connect flow does.
async fn connect_github(held: &Assigned) {
    let installation = Entropy::new()
        .uuid7(af_core::clock::now())
        .expect("an identifier");
    let installation = installation.as_str();
    let seed = InstallSeed::new(Provider::GitHub.id(), installation, &held.scene.workspace);
    seed.insert(&mut *held.connection().await)
        .await
        .expect("the installation maps to the workspace");
}

/// The fleet's triggers, read through the run.
async fn listed(held: &Assigned) -> Vec<Value> {
    let (status, page) = held
        .call(Method::GET, &fenced(held, "", held.fence), None)
        .await;
    assert_eq!(status, 200, "{page}");
    page["items"].as_array().cloned().expect("a list")
}

/// Answers `(status, error_code)` for one request as the held run.
async fn refused(
    held: &Assigned,
    method: Method,
    path: &str,
    body: Option<&Value>,
) -> (u16, Value) {
    let (status, problem) = held.call(method, path, body).await;
    (status, problem["error_code"].clone())
}

/// A webhook is added once per call, read back, and removed; a removal of a
/// trigger already gone answers 204 as well.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_fleet_adds_lists_and_removes_its_own_trigger() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let held = Box::pin(Assigned::boot(&mut supervisor, &qstash)).await;
    store_signing_secret(&held).await;
    let body = webhook(&held, "deploys");

    let (status, added) = held
        .call(Method::POST, &triggers(&held, ""), Some(&body))
        .await;
    assert_eq!(status, 201, "{added}");
    assert_eq!(
        (&added["kind"], &added["source"], &added["events"]),
        (
            &json!("webhook"),
            &json!("deploys"),
            &json!(["deploy.failed"])
        )
    );
    let address = format!("/v1/webhooks/{}/deploys", held.scene.fleet);
    let url = added["url"].as_str().expect("a webhook names its address");
    assert!(url.ends_with(&address), "{added}");

    let (status, replayed) = held
        .call(Method::POST, &triggers(&held, ""), Some(&body))
        .await;
    assert_eq!((status, &replayed["id"]), (201, &added["id"]), "{replayed}");
    let items = listed(&held).await;
    assert_eq!(items.len(), 1, "a retried call adds nothing: {items:?}");
    assert_eq!(items[0]["id"], added["id"]);

    let id = added["id"].as_str().expect("the trigger's id");
    let removal = fenced(&held, &format!("/{id}"), held.fence);
    for attempt in ["removed", "already gone"] {
        let (status, _) = held.call(Method::DELETE, &removal, None).await;
        assert_eq!(status, 204, "{attempt}");
    }
    assert_eq!(listed(&held).await, Vec::<Value>::new());
    held.finish(supervisor).await;
}

/// A GitHub App subscription needs the workspace's GitHub connected; once it
/// is, the add takes GitHub as its source and carries no delivery address.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_an_app_event_trigger_needs_github_connected() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let held = Box::pin(Assigned::boot(&mut supervisor, &qstash)).await;
    let path = triggers(&held, "");

    let unconnected = refused(&held, Method::POST, &path, Some(&app_event(&held))).await;
    assert_eq!(unconnected, (409, json!("UZ-AGT-023")));
    connect_github(&held).await;
    let (status, added) = held
        .call(Method::POST, &path, Some(&app_event(&held)))
        .await;

    assert_eq!(status, 201, "{added}");
    assert_eq!(
        (&added["kind"], &added["source"]),
        (&json!("app_event"), &json!("github"))
    );
    assert_eq!(added["repositories"], json!(["org/app"]));
    assert_eq!(added["url"], Value::Null, "only a webhook has an address");
    assert_eq!(listed(&held).await.len(), 1);
    held.finish(supervisor).await;
}

/// A webhook whose signing secret the vault lacks, a reserved source, and a
/// second webhook for one source are each refused, and only the first webhook
/// is written.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_triggers_refuse_what_the_workspace_cannot_back() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let held = Box::pin(Assigned::boot(&mut supervisor, &qstash)).await;
    let path = triggers(&held, "");

    let unsigned = refused(&held, Method::POST, &path, Some(&webhook(&held, "deploys"))).await;
    assert_eq!(unsigned, (404, json!("UZ-AGT-025")));
    store_signing_secret(&held).await;
    let reserved = refused(&held, Method::POST, &path, Some(&webhook(&held, "github"))).await;
    assert_eq!(reserved, (400, json!("UZ-REQ-001")));
    let (status, first) = held
        .call(Method::POST, &path, Some(&webhook(&held, "deploys")))
        .await;
    assert_eq!(status, 201, "{first}");
    let second = refused(&held, Method::POST, &path, Some(&webhook(&held, "deploys"))).await;

    assert_eq!(second, (409, json!("UZ-AGT-024")));
    assert_eq!(listed(&held).await.len(), 1);
    held.finish(supervisor).await;
}

/// A refusal the run verbs answer, as its status and code.
type Answer = (u16, &'static str);

/// A fence the run was not issued.
const STALE: Answer = (409, "UZ-RUN-005");
/// A run that is not this runner's.
const NOT_HELD: Answer = (404, "UZ-RUN-006");
/// A request the verb cannot read.
const UNREAD: Answer = (400, "UZ-REQ-001");

/// One refusal case: the request, then the answer it must earn.
type Case = (Method, String, Option<Value>, Answer);

/// A stale fence on each verb, a run that is not held, a read with no fence,
/// a trigger id that is not one, and an add naming a field it does not take.
fn unheld(held: &Assigned) -> Vec<Case> {
    let stale = held.fence + 1;
    let mut late = webhook(held, "deploys");
    late["fencing_token"] = json!(stale);
    let mut foreign = webhook(held, "deploys");
    foreign["fleet_id"] = json!(held.scene.fleet);
    let elsewhere = format!(
        "/v1/runners/me/runs/{UNKNOWN}/triggers?fencing_token={}",
        held.fence
    );
    let gone = fenced(held, &format!("/{UNKNOWN}"), stale);
    let unnamed = fenced(held, "/not-a-trigger", held.fence);
    vec![
        (Method::POST, triggers(held, ""), Some(late), STALE),
        (Method::GET, fenced(held, "", stale), None, STALE),
        (Method::DELETE, gone, None, STALE),
        (Method::GET, elsewhere, None, NOT_HELD),
        (Method::GET, triggers(held, ""), None, UNREAD),
        (Method::DELETE, unnamed, None, UNREAD),
        (Method::POST, triggers(held, ""), Some(foreign), UNREAD),
    ]
}

/// Every verb refuses a fence the run does not hold, a run that is not this
/// runner's, and a request it cannot read, before any trigger is touched.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_triggers_refuse_a_run_that_is_not_held() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let held = Box::pin(Assigned::boot(&mut supervisor, &qstash)).await;
    store_signing_secret(&held).await;

    for (method, path, body, (status, code)) in unheld(&held) {
        let answered = refused(&held, method.clone(), &path, body.as_ref()).await;
        assert_eq!(answered, (status, json!(code)), "{method} {path}");
    }
    assert_eq!(
        listed(&held).await,
        Vec::<Value>::new(),
        "no refusal wrote a row"
    );
    held.finish(supervisor).await;
}
