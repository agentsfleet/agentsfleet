//! A run's own tools: built over its memory, verbs and guard through
//! `guffbolt`'s builder, and only the ones its policy names.

#![expect(
    clippy::expect_used,
    reason = "test module: a tool the run does not build should fail the test loudly"
)]

use guffbolt::agents::{Agents, Caps, TOOLS};
use serde_json::json;

use super::Backends;
use crate::run::Run;
use crate::testing::{call, no_sandbox};

#[test]
fn a_run_builds_only_the_backed_tools_its_policy_names() {
    let (run, slot) = (Run::default(), no_sandbox());
    let backends = Backends::new(&run, &slot, None);

    let tools = backends
        .tools(&["memory_recall", "shell", "notify", "web_search", "teleport"])
        .expect("every backend is given");

    for built in ["memory_recall", "shell", "notify"] {
        assert!(tools.tool(built).is_some(), "{built} is built by the run");
    }
    for hosted_elsewhere in ["web_search", "teleport", "memory_store"] {
        assert!(
            tools.tool(hosted_elsewhere).is_none(),
            "{hosted_elsewhere} is not built by the run"
        );
    }
}

/// A subagent tool is built over the run's children, so a run that gave
/// none refuses one by name.
#[test]
fn a_subagent_tool_without_children_refuses_the_run() {
    let (run, slot) = (Run::default(), no_sandbox());
    let backends = Backends::new(&run, &slot, None);
    let subagent = TOOLS[0];

    let childless = backends
        .tools(&[subagent])
        .expect_err("no children were given");

    assert_eq!(childless.unhosted_tool(), Some(subagent));
}

/// A run that keeps children builds every one of `guffbolt`'s subagent
/// tools, under `guffbolt`'s names.
#[test]
fn a_run_with_children_builds_every_subagent_tool() {
    let (run, slot) = (Run::default(), no_sandbox());
    let (agents, _requests) = Agents::new(Caps { depth: 2, open: 4 });
    let backends = Backends::new(&run, &slot, None).with_agents(&agents);

    let tools = backends.tools(&TOOLS).expect("the children are given");

    for name in TOOLS {
        assert!(tools.tool(name).is_some(), "{name} is built");
    }
}

/// The memory tools a run builds read and write that run's memory: what one
/// stores, another recalls, and the push before the report carries it.
#[tokio::test]
async fn a_runs_memory_tools_share_its_memory_to_the_push() {
    let (run, slot) = (Run::default(), no_sandbox());
    let backends = Backends::new(&run, &slot, None);
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
    let (first, second, slot) = (Run::default(), Run::default(), no_sandbox());
    let (first_backends, second_backends) = (
        Backends::new(&first, &slot, None),
        Backends::new(&second, &slot, None),
    );
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
