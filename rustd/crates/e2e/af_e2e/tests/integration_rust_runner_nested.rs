//! The delegating fleet, end to end: installed from the corpus into a real
//! daemon, held by the Rust runner's real loop, its model a script, its two
//! children, which `guffbolt` drives, reading the checked-out repository
//! through the executor, and every call of the run, the children's included,
//! in the one trace the daemon stores under one counter.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly, and a step \
              indexes the JSON it just read"
)]

use agentsfleetd::supervisor::Supervisor;
use guffbolt::Chunk;
use serde_json::{Value, json};

use crate::definition_code::greeter_origin;
use crate::definition_install::install_definition;
use crate::definition_run::run_event_from;
use crate::fake_model::{Asked, FakeModel, call, say};
use crate::https::Upstream;
use crate::integration_tool_trace::stored_trace;

const FLEET: &str = "delegating-triager";
const SCRIPT: &str = "greeter/greet.sh";
const SUITE: &str = "greeter/test.sh";
/// What the script's child is told; its conversation opens with it.
const SCRIPT_TASK: &str = "summarise greeter/greet.sh, with greeter/test.sh beside it";
const SUITE_TASK: &str = "summarise greeter/test.sh, with greeter/greet.sh beside it";
const SCRIPT_SUMMARY: &str = "greet.sh prints helo";
const SUITE_SUMMARY: &str = "test.sh expects hello and fails on helo";
const TRIAGE: &str =
    "greet.sh prints helo where test.sh expects hello; the greeting is one letter short";
const FILE_READ: &str = "file_read";
const SPAWN_AGENT: &str = "spawn_agent";
const WAIT_AGENT: &str = "wait_agent";
const SUCCEEDED: &str = "succeeded";

/// What each loop says, decided from its own conversation: a child from the
/// task it opened with and how many reads it has back, the root from how
/// many results it has back: none, then the two children started, then
/// their two answers. No turn order is assumed.
fn triager(asked: &Asked) -> Vec<Chunk> {
    let opening = asked.user.first().map_or("", String::as_str);
    match (opening, asked.results.len()) {
        (SCRIPT_TASK | SUITE_TASK, 0) => vec![
            call("script", FILE_READ, json!({"path": SCRIPT})),
            call("suite", FILE_READ, json!({"path": SUITE})),
        ],
        (SCRIPT_TASK, _read) => vec![say(SCRIPT_SUMMARY)],
        (SUITE_TASK, _read) => vec![say(SUITE_SUMMARY)],
        (_event, 0) => vec![
            call(
                "s1",
                SPAWN_AGENT,
                json!({"message": SCRIPT_TASK, "tools": [FILE_READ], "fork_turns": "none"}),
            ),
            call(
                "s2",
                SPAWN_AGENT,
                json!({"message": SUITE_TASK, "tools": [FILE_READ], "fork_turns": "none"}),
            ),
        ],
        (_event, 2) => vec![
            call("w1", WAIT_AGENT, json!({"target": 1})),
            call("w2", WAIT_AGENT, json!({"target": 2})),
        ],
        (_event, _answered) => vec![say(TRIAGE)],
    }
}

/// Whether `asked` is a child's request: one opening with a task.
fn is_child(asked: &Asked) -> bool {
    matches!(
        asked.user.first().map(String::as_str),
        Some(SCRIPT_TASK | SUITE_TASK)
    )
}

/// A stored trace row as the id the runner numbered it with, its tool and
/// its status; the daemon stores the id fenced, `{fence}:{number}`.
fn row(call: &Value) -> (String, String, String) {
    let text = |field: &str| {
        call[field]
            .as_str()
            .expect("a trace row's text field")
            .to_owned()
    };
    let number = text("call_id")
        .rsplit(':')
        .next()
        .expect("a fenced call id")
        .to_owned();
    (number, text("name"), text("status"))
}

/// On `guffbolt`'s subagents: the triager starts two
/// children with `spawn_agent` and waits for each with `wait_agent`; each
/// child holds `file_read` alone, reads the checked-out repository through
/// the executor, and its answer is what its wait reads; the stored trace
/// holds the parent's four rows and the children's four `file_read` rows
/// under one counter.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_delegating_definition_roundtrip() {
    let mut supervisor = Supervisor::new();
    let scene = install_definition(&mut supervisor, FLEET, &[], None).await;
    let origins = tempfile::tempdir().expect("an origin directory");
    let origin = greeter_origin(origins.path());
    let upstream = Upstream::serve(Vec::new()).await;
    let (model, transcript) = FakeModel::deciding(triager);

    let settled = run_event_from(&scene, &scene.turn_id, &upstream, model, Some(origin)).await;

    assert_eq!(
        (settled.status.as_str(), settled.answer.as_str()),
        ("processed", TRIAGE)
    );
    let asked = transcript.asked();
    assert_eq!(
        asked.len(),
        7,
        "three root turns and two per child: {asked:#?}"
    );
    let root: Vec<&Asked> = asked.iter().filter(|turn| !is_child(turn)).collect();
    let mut offered = root[0].tools.clone();
    offered.sort_unstable();
    assert_eq!(offered, [FILE_READ, SPAWN_AGENT, WAIT_AGENT]);
    assert_eq!(
        root[2].results[2..],
        [
            json!({"status": "done", "answer": SCRIPT_SUMMARY}).to_string(),
            json!({"status": "done", "answer": SUITE_SUMMARY}).to_string(),
        ],
        "each child's answer is what its wait reads"
    );
    let children: Vec<&Asked> = asked.iter().filter(|turn| is_child(turn)).collect();
    for child in &children {
        assert_eq!(child.tools, [FILE_READ], "a child holds what it was given");
        assert_eq!(
            child.instructions, root[0].instructions,
            "a child opens with its parent's system prompt"
        );
    }
    let reads: Vec<&String> = children.iter().flat_map(|child| &child.results).collect();
    assert_eq!(reads.len(), 4, "{reads:#?}");
    assert!(
        reads[0].contains("echo helo"),
        "the child read the workspace through the executor: {reads:#?}"
    );

    let trace = stored_trace(&scene)
        .await
        .expect("the report carried a trace");
    let mut rows: Vec<(String, String, String)> = trace["calls"]
        .as_array()
        .expect("the trace lists its calls")
        .iter()
        .map(row)
        .collect();
    rows.sort_by_key(|(number, _, _)| number.parse::<u64>().unwrap_or(u64::MAX));
    let numbers: Vec<&str> = rows.iter().map(|(number, _, _)| number.as_str()).collect();
    let count = |tool: &str| rows.iter().filter(|(_, name, _)| name == tool).count();
    assert_eq!(numbers, ["1", "2", "3", "4", "5", "6", "7", "8"], "{trace}");
    assert_eq!(
        (count(SPAWN_AGENT), count(WAIT_AGENT), count(FILE_READ)),
        (2, 2, 4),
        "{trace}"
    );
    assert!(
        rows.iter().all(|(_, _, status)| status == SUCCEEDED),
        "{trace}"
    );
    assert_eq!(trace["omitted_call_count"], 0, "{trace}");

    supervisor.shutdown().await;
    scene.cleanup().await;
}
