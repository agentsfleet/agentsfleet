//! The write-bound bundles, end to end: each repairer reconciles, reads, and
//! writes through the Git Data API under the rules the daemon compiled for its
//! lease — the one branch it named, a draft against the trusted base — with a
//! token minted through the real grant check and broker.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly, and a step \
              indexes the JSON it just read"
)]

use afd_gate::policy::repair::branch_for;
use afd_wire::event::EventType;
use afr_tools::ToolErrorCode;
use agentsfleetd::supervisor::Supervisor;
use hyper::Method;
use serde_json::{Value, json};

use crate::bundle_install::{Secret, install_bundle};
use crate::bundle_repair::{
    DIAGNOSIS_ONLY, DRAFT_URL, GITHUB, HEAD_SHA, Repo, file_at_head, github_auth, head_reads,
    routes, script,
};
use crate::bundle_run::{assert_token_stayed_on_the_wire, body_at, posts, run_event};
use crate::fake_model::FakeModel;
use crate::https::{Reply, Route, Upstream};

/// `ci-repairer`'s binding.
const LINKWARDEN: Repo = Repo {
    name: "agentsfleet/linkwarden",
    base: "dev",
};
/// `incident-repairer`'s binding.
const AGENTSFLEET: Repo = Repo {
    name: "agentsfleet/agentsfleet",
    base: "main",
};
/// The incident repairer's telemetry hosts, as its policy lists them.
const ELASTIC: &str = "demo.es.us-east-1.aws.elastic.cloud";
const GRAFANA: &str = "demo-grafana.internal";
const QUERY: &str = "/_query";
const ANNOTATIONS: &str = "/api/annotations";
/// The five Git Data writes and the draft, in the order a repair makes them.
const WRITES: [&str; 5] = ["git/blobs", "git/trees", "git/commits", "git/refs", "pulls"];

/// `repo`'s write paths, in [`WRITES`] order.
fn writes(repo: Repo) -> Vec<String> {
    WRITES.iter().map(|rest| repo.path(rest)).collect()
}

/// A run's draft and ref went to the branch the daemon named for `event_id`,
/// as a draft against `repo`'s base, and the model was told that branch.
fn assert_repaired_on_named_branch(
    repo: Repo,
    event_id: &str,
    seen: &[crate::https::Seen],
    asked: &[crate::fake_model::Asked],
) {
    let branch = branch_for(event_id);
    assert!(
        asked[0]
            .instructions
            .contains(&format!("repair branch: {branch}")),
        "the prompt carries the trusted repair context naming {branch}: {}",
        asked[0].instructions
    );
    assert_eq!(
        body_at(seen, &repo.path("git/refs"))["ref"],
        json!(format!("refs/heads/{branch}"))
    );
    let draft = body_at(seen, &repo.path("pulls"));
    assert_eq!(
        (&draft["head"], &draft["base"], &draft["draft"]),
        (&json!(branch), &json!(repo.base), &json!(true))
    );
    let refused = format!("[{}]", ToolErrorCode::RequestPolicyNotAllowed.as_str());
    let results = &asked.last().expect("the model was asked").results;
    assert!(
        results.iter().any(|result| result.starts_with(&refused)),
        "a ref outside the lock reaches the model as a refusal: {results:#?}"
    );
}

/// Dimension 6.2. The repairer reconciles, re-reads the head, writes blob,
/// tree, commit, the named ref and one draft — a ref at the base is refused
/// before it leaves — and a second run that finds its draft ends with the
/// link and writes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_ci_repairer_opens_one_draft_pull_request() {
    let mut supervisor = Supervisor::new();
    let run = install_bundle(&mut supervisor, "ci-repairer", &[], None).await;
    let upstream = Upstream::serve(routes(LINKWARDEN, None, file_at_head())).await;
    let (model, transcript) = FakeModel::deciding(script(LINKWARDEN, head_reads(LINKWARDEN)));

    let settled = run_event(&run, &run.event_id, &upstream, model).await;

    assert_eq!(settled.status, "processed");
    assert!(settled.answer.contains(DRAFT_URL), "{}", settled.answer);
    let (seen, asked) = (upstream.seen(), transcript.asked());
    assert_eq!(posts(&seen), writes(LINKWARDEN));
    assert_repaired_on_named_branch(LINKWARDEN, &run.event_id, &seen, &asked);
    assert_token_stayed_on_the_wire(&seen, &asked);

    let second = run.enqueue_event(EventType::Chat).await;
    let found = branch_for(&second);
    let again = Upstream::serve(routes(LINKWARDEN, Some(&found), file_at_head())).await;
    let (model, _transcript) = FakeModel::deciding(script(LINKWARDEN, head_reads(LINKWARDEN)));
    let settled = run_event(&run, &second, &again, model).await;
    assert_eq!(settled.status, "processed");
    assert!(settled.answer.contains(DRAFT_URL), "{}", settled.answer);
    assert!(
        posts(&again.seen()).is_empty(),
        "a found draft ends the run before any write"
    );

    supervisor.shutdown().await;
    run.cleanup().await;
}

/// The incident repairer's two telemetry credentials.
fn telemetry() -> [Secret; 2] {
    [
        Secret {
            name: "elastic",
            body: json!({"host": ELASTIC, "api_key": "es_fixture_key"}),
        },
        Secret {
            name: "grafana",
            body: json!({"host": GRAFANA, "token": "glsa_fixture_editor"}),
        },
    ]
}

/// The telemetry, the compare and the head the repairer reads before writing.
fn incident_reads() -> Vec<Value> {
    let query =
        json!({"query": "FROM logs-* | WHERE service == \"api\" | STATS errors = COUNT(*)"});
    let mut reads = vec![
        json!({"url": format!("https://${{secrets.elastic.host}}{QUERY}"), "method": "POST",
               "headers": {"Authorization": "ApiKey ${secrets.elastic.api_key}",
                           "Content-Type": "application/json"},
               "body": query.to_string()}),
        json!({"url": format!("https://${{secrets.grafana.host}}{ANNOTATIONS}"),
               "headers": {"Authorization": "Bearer ${secrets.grafana.token}"}}),
        json!({"url": format!("https://{GITHUB}{}",
                              AGENTSFLEET.path(&format!("compare/main~1...{HEAD_SHA}"))),
               "headers": github_auth()}),
    ];
    reads.extend(head_reads(AGENTSFLEET));
    reads
}

/// GitHub, Elasticsearch and Grafana for one incident run, `contents`
/// answering the file read.
fn incident_upstream(contents: Reply) -> Vec<Route> {
    let mut all = routes(AGENTSFLEET, None, contents);
    all.extend([
        Route::new(
            ELASTIC,
            Method::POST,
            QUERY,
            vec![Reply::json(
                200,
                &json!({"columns": [{"name": "errors"}], "values": [[42]]}),
            )],
        ),
        Route::new(
            GRAFANA,
            Method::GET,
            ANNOTATIONS,
            vec![Reply::json(
                200,
                &json!([{"text": format!("deploy main@{HEAD_SHA}")}]),
            )],
        ),
        Route::new(
            GITHUB,
            Method::GET,
            &AGENTSFLEET.path(&format!("compare/main~1...{HEAD_SHA}")),
            vec![Reply::json(
                200,
                &json!({"files": [{"filename": crate::bundle_repair::FILE}]}),
            )],
        ),
    ]);
    all
}

/// Dimension 6.4. After a full read the incident repairer ships five writes
/// and one draft, its read-only telemetry hosts admitting only the listed
/// query; after a partial read — the file answers 403 — it ends
/// diagnosis-only and nothing is written.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_incident_repairer_ships_or_stops() {
    let mut supervisor = Supervisor::new();
    let run = install_bundle(&mut supervisor, "incident-repairer", &telemetry(), None).await;
    let upstream = Upstream::serve(incident_upstream(file_at_head())).await;
    let (model, transcript) = FakeModel::deciding(script(AGENTSFLEET, incident_reads()));

    let settled = run_event(&run, &run.event_id, &upstream, model).await;

    assert_eq!(settled.status, "processed");
    assert!(settled.answer.contains(DRAFT_URL), "{}", settled.answer);
    let (seen, asked) = (upstream.seen(), transcript.asked());
    let mut shipped = vec![QUERY.to_owned()];
    shipped.extend(writes(AGENTSFLEET));
    assert_eq!(posts(&seen), shipped);
    let key = seen
        .iter()
        .find(|request| request.host == ELASTIC)
        .and_then(|request| request.headers.get("authorization"))
        .and_then(|value| value.to_str().ok());
    assert_eq!(key, Some("ApiKey es_fixture_key"));
    assert_repaired_on_named_branch(AGENTSFLEET, &run.event_id, &seen, &asked);
    assert_token_stayed_on_the_wire(&seen, &asked);

    let second = run.enqueue_event(EventType::Chat).await;
    let forbidden = Reply::json(
        403,
        &json!({"message": "Resource not accessible by integration"}),
    );
    let partial = Upstream::serve(incident_upstream(forbidden)).await;
    let (model, transcript) = FakeModel::deciding(script(AGENTSFLEET, incident_reads()));
    let settled = run_event(&run, &second, &partial, model).await;
    let read = transcript.asked().last().map(|turn| turn.results.clone());
    assert_eq!(
        (settled.status.as_str(), settled.answer.as_str()),
        ("processed", DIAGNOSIS_ONLY),
        "{read:#?}"
    );
    assert_eq!(
        posts(&partial.seen()),
        [QUERY],
        "no write follows a partial read"
    );

    supervisor.shutdown().await;
    run.cleanup().await;
}
