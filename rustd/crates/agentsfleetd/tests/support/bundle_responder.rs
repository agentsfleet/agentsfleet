//! The read-bound responder's fixtures: GitHub and Grafana as a failed
//! linkwarden run left them, and the investigation `ci-responder/SKILL.md`
//! orders, as the bundle suite scripts it.

use afr_providers::Chunk;
use hyper::Method;
use serde_json::json;

use crate::bundle_install::Secret;
use crate::bundle_repair::{GITHUB, github_auth};
use crate::fake_model::{call, http, say};
use crate::https::{Reply, Route};

/// The run, jobs and commits the responder reads, under its one repository.
pub(crate) const RUN: &str = "/repos/agentsfleet/linkwarden/actions/runs/101";
const JOBS: &str = "/repos/agentsfleet/linkwarden/actions/runs/101/jobs";
const JOB_LOG: &str = "/repos/agentsfleet/linkwarden/actions/jobs/202/logs";
const COMMITS: &str = "/repos/agentsfleet/linkwarden/commits";
pub(crate) const RUN_URL: &str = "https://github.com/agentsfleet/linkwarden/actions/runs/101";
pub(crate) const FAILED_STEP: &str = "Run unit tests";
/// Where GitHub's 302 sends a job log, signature and all; the model reads the
/// origin alone.
pub(crate) const LOG_STORE: &str = "https://pipelines.actions.githubusercontent.com";
pub(crate) const LOG_SIGNATURE: &str = "sig=fixture-signature";
/// The bundle's Grafana stack, as its install binding names it, and the
/// Viewer token sealed for it.
pub(crate) const GRAFANA: &str = "grafana.example.net";
pub(crate) const GRAFANA_TOKEN: &str = "glsa_fixture_viewer";
const LOKI: &str = "/api/datasources/proxy/uid/loki-uid/loki/api/v1/query_range";
pub(crate) const LOKI_LINE: &str = "linkwarden-api TypeError: cannot read properties of undefined";
const ANNOTATIONS: &str = "/api/annotations";
/// What the responder answers and remembers.
pub(crate) const DIAGNOSIS: &str = "Run 101 failed at Run unit tests; job log unavailable (302); \
                         Loki shows the TypeError; annotations unreadable.";
pub(crate) const MEMORY_KEY: &str = "ci:linkwarden:101";
/// The Grafana credential the responder declares.
pub(crate) fn grafana() -> Secret {
    Secret {
        name: "grafana",
        body: json!({"host": GRAFANA, "token": GRAFANA_TOKEN}),
    }
}

/// A GET to `path` on GitHub with the minted credential.
pub(crate) fn github_get(id: &str, path: &str) -> Chunk {
    http(
        id,
        json!({"url": format!("https://{GITHUB}{path}"), "headers": github_auth()}),
    )
}

/// A GET to `path_and_query` on the sealed Grafana host, its token in place.
fn grafana_get(id: &str, path_and_query: &str) -> Chunk {
    http(
        id,
        json!({"url": format!("https://${{secrets.grafana.host}}{path_and_query}"),
                    "headers": {"Authorization": "Bearer ${secrets.grafana.token}"}}),
    )
}

/// GitHub and Grafana as a failed linkwarden run left them.
pub(crate) fn responder_upstream() -> Vec<Route> {
    let ok = |body: serde_json::Value| vec![Reply::json(200, &body)];
    vec![
        Route::new(
            GITHUB,
            Method::GET,
            RUN,
            ok(json!({"id": 101, "html_url": RUN_URL,
            "conclusion": "failure", "head_sha": "c0ffee1"})),
        ),
        Route::new(
            GITHUB,
            Method::GET,
            JOBS,
            ok(json!({"jobs": [{"id": 202, "name": "test",
            "conclusion": "failure", "steps": [{"name": FAILED_STEP, "conclusion": "failure"}]}]})),
        ),
        Route::new(
            GITHUB,
            Method::GET,
            JOB_LOG,
            vec![Reply::found(&format!(
                "{LOG_STORE}/logs/202?{LOG_SIGNATURE}"
            ))],
        ),
        Route::new(
            GITHUB,
            Method::GET,
            COMMITS,
            ok(json!([{"sha": "c0ffee1"}])),
        ),
        Route::new(
            GRAFANA,
            Method::GET,
            "/api/datasources",
            ok(json!([{"uid": "loki-uid", "type": "loki"}])),
        ),
        Route::new(
            GRAFANA,
            Method::GET,
            LOKI,
            ok(json!({"data": {"result": [{"values": [["1", LOKI_LINE]]}]}})),
        ),
        Route::new(
            GRAFANA,
            Method::GET,
            ANNOTATIONS,
            vec![Reply::cut(r#"[{"id": 1, "text""#)],
        ),
    ]
}

/// The responder's investigation, as `ci-responder/SKILL.md` orders it.
pub(crate) fn responder_turns() -> Vec<Vec<Chunk>> {
    vec![
        vec![
            call("recall", "memory_recall", json!({"query": "linkwarden"})),
            github_get("run", RUN),
            github_get("jobs", JOBS),
        ],
        vec![github_get("log", JOB_LOG), github_get("commits", COMMITS)],
        vec![
            grafana_get("datasources", "/api/datasources"),
            grafana_get(
                "loki",
                &format!("{LOKI}?query=%7Bapp%3D%22linkwarden%22%7D&direction=backward"),
            ),
            grafana_get("annotations", ANNOTATIONS),
        ],
        vec![call(
            "store",
            "memory_store",
            json!({"key": MEMORY_KEY, "content": DIAGNOSIS, "category": "core"}),
        )],
        vec![say(DIAGNOSIS)],
    ]
}
