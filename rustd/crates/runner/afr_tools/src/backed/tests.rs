//! A run's own tools: built over its memory, verbs and guard through
//! `guffbolt`'s builder, and only the ones its policy names.

#![expect(
    clippy::expect_used,
    reason = "test module: a tool the run does not build should fail the test loudly"
)]

use serde_json::json;

use super::Backends;
use crate::run::Run;
use crate::testing::call;

#[test]
fn a_run_builds_only_the_backed_tools_its_policy_names() {
    let run = Run::default();
    let backends = Backends::new(&run, None);

    let tools = backends
        .tools(&["memory_recall", "shell", "notify", "web_search", "teleport"])
        .expect("every backend is given");

    assert!(tools.tool("memory_recall").is_some());
    assert!(tools.tool("notify").is_some());
    for hosted_elsewhere in ["shell", "web_search", "teleport", "memory_store"] {
        assert!(
            tools.tool(hosted_elsewhere).is_none(),
            "{hosted_elsewhere} is not built by the run"
        );
    }
}

/// The memory tools a run builds read and write that run's memory: what one
/// stores, another recalls, and the push before the report carries it.
#[tokio::test]
async fn a_runs_memory_tools_share_its_memory_to_the_push() {
    let run = Run::default();
    let backends = Backends::new(&run, None);
    let tools = backends
        .tools(&["memory_store", "memory_recall"])
        .expect("every backend is given");
    let (store, recall) = (
        tools.tool("memory_store").expect("built"),
        tools.tool("memory_recall").expect("built"),
    );

    let stored = call(
        store,
        &run,
        json!({"key": "deploy:target", "content": "staging first"}),
    )
    .await;
    let recalled = call(recall, &run, json!({"query": "deploy"})).await;
    let pending = run.memory.pending().await;

    assert_eq!(stored.text, "stored deploy:target");
    assert_eq!(recalled.text, "deploy:target (core): staging first");
    assert_eq!(pending.len(), 1, "one store is one pending push");
    assert_eq!(
        pending.first().map(|delta| delta.key.as_ref()),
        Some("deploy:target")
    );
}

/// Two runs build two sets of tools, each over its own memory.
#[tokio::test]
async fn two_runs_keep_their_memories_apart() {
    let (first, second) = (Run::default(), Run::default());
    let (first_backends, second_backends) =
        (Backends::new(&first, None), Backends::new(&second, None));
    let first_tools = first_backends.tools(&["memory_store"]).expect("built");
    let second_tools = second_backends.tools(&["memory_recall"]).expect("built");

    call(
        first_tools.tool("memory_store").expect("built"),
        &first,
        json!({"key": "k", "content": "first run's"}),
    )
    .await;
    let recalled = call(
        second_tools.tool("memory_recall").expect("built"),
        &second,
        json!({"query": "k"}),
    )
    .await;

    assert_eq!(recalled.text, "nothing remembered matches k");
    assert_eq!(second.memory.pending().await, Vec::new());
}
