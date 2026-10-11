//! The daemon suite's scripted external model, reused by the kernel lane.

/// Shared tool, call and field spellings in the fixture.
const CRON_RUN: &str = "cron_run";
const WRITE: &str = "write";
const PUSH: &str = "push";
const SCHEDULE: &str = "schedule";
const ARGS: &str = "args";
const GET: &str = "GET";
const LOG: &str = "log";
const README: &str = "widget/README.md";
const CONTENT: &str = "content";
const GIT: &str = "git";
const COMMIT: &str = "commit";
const PATH: &str = "path";
const SCHEDULE_FIELD: &str = "schedule_id";
const MESSAGE: &str = "message";
const ORIGIN: &str = "origin";

use crate::supervisor_plane::SCHEDULE_ID;
use afd_wire::activity::StreamTextKind;
use afr_providers::{Call, Chunk};
use serde_json::{Value, json};

#[path = "../../../agentsfleetd/tests/support/fake_model.rs"]
#[allow(
    dead_code,
    reason = "shared model support: kernel trials use scripted turns and observations"
)]
mod existing;
pub(crate) use existing::Asked;

/// The installed instructions accompanying the fixture's lease.
pub(crate) const INSTRUCTIONS: &str = "Read Grafana, remember findings, and report to the thread.";
pub(crate) const STEER: &str = "Check the fleet and commit the workspace fix.";
pub(crate) const ANSWER: &str = "kernel caller proof complete";

/// One call per turn keeps state-changing calls ordered.
pub(crate) fn model(calls: &[Call]) -> (existing::FakeModel, existing::Transcript) {
    let answer = Chunk::Text {
        kind: StreamTextKind::Answer,
        text: ANSWER.to_owned(),
    };
    let turns = calls
        .iter()
        .cloned()
        .map(|call| vec![Chunk::Call(call)])
        .chain([vec![answer]])
        .collect();
    existing::FakeModel::new(turns)
}

pub(crate) fn call(id: &str, name: &str, arguments: Value) -> Call {
    Call {
        id: id.to_owned(),
        name: name.to_owned(),
        arguments,
    }
}

pub(crate) fn edit_calls() -> Vec<Call> {
    vec![
        grafana_call(),
        call(
            "confinement",
            "shell",
            json!({ "command":
                "cat /proc/self/status; (echo x > /opt/outside) 2>/dev/null && echo leaked || echo confined; env; cat widget/.git/config"
            }),
        ),
        call(
            WRITE,
            "file_write",
            json!({ PATH: README, CONTENT: "steered fix\n" }),
        ),
        call("read", "file_read", json!({ PATH: README })),
        call(
            COMMIT,
            GIT,
            json!({ ARGS: [COMMIT, "--all", "--message", "steered fix"] }),
        ),
        call(LOG, GIT, json!({ ARGS: [LOG, "-1", "--format=%an %s"] })),
    ]
}

pub(crate) fn supervisor_calls() -> Vec<Call> {
    let mut calls = mutations();
    calls.insert(
        0,
        call(
            "recall",
            "memory_recall",
            json!({ "query": "prior", "limit": 1 }),
        ),
    );
    calls.insert(
        1,
        call(
            "store",
            "memory_store",
            json!({ "key": "finding", CONTENT: "kernel caller verified" }),
        ),
    );
    calls.push(call(
        "run",
        CRON_RUN,
        json!({ SCHEDULE_FIELD: SCHEDULE_ID }),
    ));
    calls.push(grafana_call());
    calls
}

fn grafana_call() -> Call {
    call(
        "grafana",
        "http_request",
        request(GET, "https://${secrets.grafana.host}/api/search"),
    )
}

pub(crate) fn refused_calls() -> Vec<Call> {
    vec![
        call(PUSH, GIT, json!({ ARGS: [PUSH, ORIGIN, "main"] })),
        call(
            WRITE,
            "http_request",
            request("POST", "https://${secrets.grafana.host}/api/search"),
        ),
        call(
            ORIGIN,
            "http_request",
            request(GET, "https://other.example.test/api/search"),
        ),
        call("invalid", GIT, json!({ ARGS: true })),
        call(
            SCHEDULE,
            CRON_RUN,
            json!({ SCHEDULE_FIELD: "../another-fleet" }),
        ),
    ]
}
pub(crate) fn mutations() -> Vec<afr_providers::Call> {
    vec![
        call(
            SCHEDULE,
            "cron_add",
            json!({ "cron": "0 9 * * 1", "timezone": "Asia/Kolkata", MESSAGE: "check fleet" }),
        ),
        call(MESSAGE, MESSAGE, json!({ "text": "fleet checked" })),
    ]
}

fn request(method: &str, url: &str) -> Value {
    json!({ "method": method, "url": url, "headers": { "Authorization": "Bearer ${secrets.grafana.token}" } })
}
