//! What the model reads from the generic tools names no host product: every
//! tool the builder builds, its name, description and parameters, the
//! answers and refusals they read back, and the values memory's visibility
//! takes.

#![expect(
    clippy::expect_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::Arc;

use serde_json::{Value, json};

use crate::agents::{Agents, Caps};
use crate::tools::env::Env as _;
use crate::tools::fake::Fake;
use crate::tools::files::FileRead;
use crate::tools::mcp::{ListMcpResourceTemplates, ListMcpResources, ReadMcpResource, Server};
use crate::tools::memory::{Memory, MemoryForget, MemoryStoreTool, Reach};
use crate::tools::outbound::Allowlist;
use crate::tools::process::Git;
use crate::tools::search::{Deferred, ToolSearch};
use crate::tools::{ContextTool, Local, LocalMemory, ToolsConfig, Typed, code};
use crate::{CallRef, Describe as _, Tool as _, ToolSpec};

/// The words that name a host product rather than the harness.
const HOST_WORDS: [&str; 4] = ["agentsfleet", "daemon", "fleet", "workspace"];

/// The host words `text` holds: each of its words, split at anything that is
/// neither a letter nor a digit, that starts with one, ignoring case. A code
/// such as `workspace_memory_not_granted` splits at its underscores.
fn host_words(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|word| HOST_WORDS.iter().any(|host| word.starts_with(host)))
        .collect()
}

/// Every string in `value`, each object key included: a property's name,
/// its description and every enum value the model is told.
fn strings(value: &Value) -> Vec<String> {
    match value {
        Value::String(text) => vec![text.clone()],
        Value::Array(items) => items.iter().flat_map(strings).collect(),
        Value::Object(fields) => fields
            .iter()
            .flat_map(|(key, field)| std::iter::once(key.clone()).chain(strings(field)))
            .collect(),
        Value::Null | Value::Bool(_) | Value::Number(_) => Vec::new(),
    }
}

/// Everything the model is told about one tool.
fn model_text(spec: ToolSpec<'_>) -> Vec<String> {
    [spec.name.to_owned(), spec.description.to_owned()]
        .into_iter()
        .chain(strings(spec.parameters))
        .collect()
}

/// The values a schema lets the model write: each `const`, and each `enum`
/// member, wherever they sit in it.
fn values(schema: &Value) -> Vec<String> {
    match schema {
        Value::Array(items) => items.iter().flat_map(values).collect(),
        Value::Object(fields) => fields
            .iter()
            .flat_map(|(key, field)| match (key.as_str(), field) {
                ("const", Value::String(value)) => vec![value.clone()],
                ("enum", Value::Array(members)) => members.iter().flat_map(strings).collect(),
                _ => values(field),
            })
            .collect(),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => Vec::new(),
    }
}

/// Every tool the crate builds: every built-in over a backend for each, which
/// `*` offers, the seven subagent tools among them, then `tool_search` and the
/// three MCP resource tools. An MCP server's own tools are the server's
/// words, not the crate's.
fn built_in<'run>(
    local: &'run Local,
    fake: &'run Fake,
    agents: &'run Agents,
) -> Vec<Box<ContextTool<'run>>> {
    let servers: Vec<Arc<Server>> = Vec::new();
    local
        .builder()
        .with_scheduler(fake)
        .with_channel(fake)
        .with_notifier(fake)
        .with_agents(agents)
        .with_tool(Typed::new(ToolSearch::new(Arc::new(Deferred::default()))))
        .with_tool(Typed::new(ListMcpResources::new(servers.clone())))
        .with_tool(Typed::new(ListMcpResourceTemplates::new(servers.clone())))
        .with_tool(Typed::new(ReadMcpResource::new(servers)))
        .build(&ToolsConfig::new(["*"]).expect("one pattern"))
        .expect("every backend is given")
        .into_offered()
}

/// Where the one call each case makes sits: the first of step 1.
fn first_call() -> CallRef {
    CallRef {
        step: 1,
        index: 0,
        call_id: "call-1".to_owned(),
    }
}

/// A standalone run over the temporary directory, open to any public host.
fn local() -> Local {
    Local::new("run-1", std::env::temp_dir(), Allowlist::Any)
        .expect("the temporary directory exists")
}

#[test]
fn guffbolt_tool_specs_name_no_host_product() {
    assert_eq!(
        host_words("this Fleet's [workspace_memory_not_granted] from agentsfleetd's daemon"),
        ["fleet", "workspace", "agentsfleetd", "daemon"],
        "the check finds each host word it looks for"
    );
    let (local, fake) = (local(), Fake::answering(Ok(String::new())));
    let (agents, _requests) = Agents::new(Caps { depth: 2, open: 4 });
    let tools = built_in(&local, &fake, &agents);

    let found: Vec<String> = tools
        .iter()
        .flat_map(|tool| {
            let spec = tool.spec();
            model_text(spec)
                .iter()
                .flat_map(|text| host_words(text))
                .map(|word| format!("{}: {word}", spec.name))
                .collect::<Vec<_>>()
        })
        .collect();

    assert_eq!(
        tools.len(),
        49,
        "the 45 built-ins, the seven subagent tools among them, tool_search and the three MCP \
         resource tools are each checked"
    );
    assert_eq!(
        found,
        Vec::<String>::new(),
        "a tool's spec names a host product"
    );
}

#[tokio::test]
async fn refusals_and_answers_name_no_host_product() {
    let mut local = local();
    local.memory = LocalMemory::seeded(vec![Memory {
        key: "deploy:target".to_owned(),
        content: "staging first".to_owned(),
        category: "core".to_owned(),
        reach: Reach::Private,
    }]);
    let at = first_call();
    let cx = local.context(&at);
    let (read, git) = (FileRead::new(&local.env), Git::new(&local.env, None));

    let answers = [
        (
            Typed::new(read)
                .call(&json!({"path": "../outside"}), cx)
                .await,
            "[path_not_allowed] ../outside leaves the working directory",
        ),
        (
            Typed::new(read).call(&json!({"path": "."}), cx).await,
            "[invalid_arguments] path must name a file in the working directory",
        ),
        (
            Typed::new(git).call(&json!({"args": ["push"]}), cx).await,
            "[subcommand_not_allowed] git push reaches a remote, which this environment does not \
             reach",
        ),
        (
            Typed::new(MemoryForget::new(&local.memory))
                .call(&json!({"key": "deploy:target"}), cx)
                .await,
            "deploy:target is forgotten for the rest of this run; the stored copy stays until a \
             store under the same key replaces it",
        ),
    ];
    let left = local
        .env
        .read_file("../outside", 1)
        .await
        .expect_err("a path that climbs out of the root is refused");

    for (output, expected) in answers {
        assert_eq!(output.text, expected);
    }
    assert_eq!(left.message(), "the path leaves the working directory");
    assert_eq!(
        code::SHARED_MEMORY_NOT_GRANTED.as_str(),
        "shared_memory_not_granted"
    );
}

#[tokio::test]
async fn a_memory_is_private_unless_the_model_names_it_shared() {
    let local = local();
    let store = Typed::new(MemoryStoreTool::new(&local.memory));
    let visibility = store
        .spec()
        .parameters
        .pointer("/properties/visibility")
        .cloned()
        .expect("memory_store takes a visibility");

    for (arguments, stored) in [
        (json!({"key": "a", "content": "one"}), "stored a"),
        (
            json!({"key": "b", "content": "two", "visibility": "shared"}),
            "stored b",
        ),
        (
            json!({"key": "c", "content": "three", "visibility": "private"}),
            "stored c",
        ),
    ] {
        let at = first_call();
        assert_eq!(
            store.call(&arguments, local.context(&at)).await.text,
            stored
        );
    }

    assert_eq!(values(&visibility), ["private", "shared"]);
    let reaches: Vec<(String, Reach)> = local
        .memory
        .entries()
        .into_iter()
        .map(|memory| (memory.key, memory.reach))
        .collect();
    assert_eq!(
        reaches,
        [
            ("a".to_owned(), Reach::Private),
            ("b".to_owned(), Reach::Shared),
            ("c".to_owned(), Reach::Private),
        ]
    );
}

#[tokio::test]
async fn a_visibility_outside_private_and_shared_is_refused() {
    let local = local();
    let store = Typed::new(MemoryStoreTool::new(&local.memory));
    let at = first_call();

    for named in ["workspace", "fleet", "Shared", "", "共有"] {
        let refused = store
            .call(
                &json!({"key": "a", "content": "one", "visibility": named}),
                local.context(&at),
            )
            .await;

        assert_eq!(refused.error_code, Some(code::INVALID_ARGUMENTS), "{named}");
        assert_eq!(
            refused.text,
            format!(
                "[invalid_arguments] unknown variant `{named}`, expected `private` or `shared`"
            )
        );
    }
    assert_eq!(
        local.memory.entries(),
        Vec::<Memory>::new(),
        "nothing is stored"
    );
}
