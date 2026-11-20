//! A loop over tools `Tools::builder()` built: backends given with the
//! `with_*` setters, a tool of the embedder's own beside them, and every way
//! the builder or a call refuses.

use guffbolt::agents::{Agents, Caps};
use guffbolt::tools::memory::Memory;
use guffbolt::tools::{
    Backend, Context, ContextTool, LocalChannel, LocalEnv, LocalMemory, LocalScheduler, Sessions,
    ToolsConfig, built_in,
};
use guffbolt::{Describe, DriveState, Plain, Tool, ToolFuture, ToolOutput, ToolSpec, Tools};
use serde_json::{Value, json};

use crate::family::{Family, ROOT, answered, user};
use crate::script::{Play, Script, call, results};

const CAPS: Caps = Caps {
    depth: 1,
    open: 1,
    started: 1,
};

/// A tool of the embedder's own, answering with the call's place.
#[derive(Debug)]
struct Echo {
    name: &'static str,
    parameters: Value,
}

impl Describe for Echo {
    fn spec(&self) -> ToolSpec<'_> {
        ToolSpec {
            name: self.name,
            description: "Echoes its call.",
            parameters: &self.parameters,
        }
    }
}

impl<'c> Tool<Context<'c>> for Echo {
    fn call<'a>(&'a self, arguments: &'a Value, cx: Context<'c>) -> ToolFuture<'a>
    where
        Context<'c>: 'a,
    {
        let said = format!(
            "{} {} at step {} call {}",
            self.name, arguments, cx.call_ref.step, cx.call_ref.index
        );
        Box::pin(async move { ToolOutput::succeeded(said) })
    }
}

fn echo(name: &'static str) -> Echo {
    Echo {
        name,
        parameters: json!({"type": "object"}),
    }
}

#[tokio::test]
async fn a_run_over_built_tools_calls_them_and_ends() {
    let memory = LocalMemory::default();
    let config = ToolsConfig::new(["memory_*", "echo"]).expect("two entries");
    let tools: Tools<ContextTool<'_>> = Tools::builder()
        .with_memory(&memory)
        .with_tool(echo("echo"))
        .build(&config)
        .expect("built");
    let (agents, requests) = Agents::new(CAPS);
    let script = Script::default().plays(
        ROOT,
        [
            call(
                "memory_store",
                json!({"key": "deploy:window", "content": "Fridays after six"}),
            ),
            Play::Call(vec![
                ("memory_recall", json!({"query": "DEPLOY"})),
                ("echo", json!({"n": 1})),
            ]),
            Play::Say("deploys go out on Fridays after six"),
        ],
    );
    let family = Family {
        agents,
        script,
        tools: &tools,
    };

    let (ending, transcript) = family
        .run(
            requests,
            vec![user("when do we deploy?")],
            &mut Plain,
            &mut DriveState::default(),
        )
        .await;

    assert_eq!(
        answered(&ending),
        Some("deploys go out on Fridays after six")
    );
    assert_eq!(
        results(&transcript),
        [
            "stored deploy:window",
            "deploy:window (core): Fridays after six",
            r#"echo {"n":1} at step 2 call 1"#,
        ]
    );
    assert_eq!(
        memory.entries(),
        [Memory {
            key: "deploy:window".to_owned(),
            content: "Fridays after six".to_owned(),
            category: "core".to_owned(),
            reach: guffbolt::tools::memory::Reach::Private,
        }]
    );
    assert_eq!(
        family
            .script
            .sent_by(ROOT)
            .first()
            .map(|sent| sent.tools.clone()),
        Some(
            [
                "memory_store",
                "memory_recall",
                "memory_list",
                "memory_forget",
                "echo"
            ]
            .map(str::to_owned)
            .to_vec()
        ),
        "offered in the config's order"
    );
}

/// A call whose arguments break the tool's schema, and one to a name the run
/// holds no tool for, answer the model; the loop goes on.
#[tokio::test]
async fn a_refused_call_through_the_builder_answers_the_model_and_the_loop_continues() {
    let memory = LocalMemory::default();
    let config = ToolsConfig::new(["memory_store"]).expect("a name");
    let tools = Tools::builder()
        .with_memory(&memory)
        .build(&config)
        .expect("built");
    let (agents, requests) = Agents::new(CAPS);
    let script = Script::default().plays(
        ROOT,
        [
            Play::Call(vec![
                ("memory_store", json!({"key": "k"})),
                (
                    "memory_store",
                    json!({"key": "k", "content": "c", "colour": "red"}),
                ),
                ("memory_store", json!("not an object")),
                ("memory_recall", json!({"query": "k"})),
            ]),
            call(
                "memory_store",
                json!({"key": "k", "content": "\u{1F680} launch"}),
            ),
            Play::Say("stored it the second time"),
        ],
    );
    let family = Family {
        agents,
        script,
        tools: &tools,
    };

    let (ending, transcript) = family
        .run(
            requests,
            vec![user("go")],
            &mut Plain,
            &mut DriveState::default(),
        )
        .await;

    assert_eq!(answered(&ending), Some("stored it the second time"));
    let read = results(&transcript);
    assert_eq!(read.len(), 5);
    assert_eq!(
        read.first().map(String::as_str),
        Some("[invalid_arguments] missing field `content`")
    );
    assert!(
        read.get(1).is_some_and(
            |refusal| refusal.starts_with("[invalid_arguments] unknown field `colour`")
        ),
        "{read:?}"
    );
    assert!(
        read.get(2)
            .is_some_and(|refusal| refusal.starts_with("[invalid_arguments] invalid type: string")),
        "{read:?}"
    );
    assert_eq!(
        read.get(3).map(String::as_str),
        Some("[tool_not_offered] memory_recall is not one of this run's tools"),
        "a tool the config did not name is not held"
    );
    assert_eq!(read.get(4).map(String::as_str), Some("stored k"));
    assert_eq!(
        memory
            .entries()
            .iter()
            .map(|entry| entry.content.as_str())
            .collect::<Vec<_>>(),
        ["\u{1F680} launch"],
        "only the call that parsed stored anything"
    );
}

/// Every built-in named whose backend the builder was not given stops the
/// build, naming the tool and the backend.
#[test]
fn a_missing_backend_refuses_at_build() {
    let cases = [
        ("shell", Backend::Env),
        ("file_read", Backend::Env),
        ("exec_command", Backend::Env),
        ("http_request", Backend::Outbound),
        ("web_fetch", Backend::Outbound),
        ("memory_store", Backend::Memory),
        ("cron_add", Backend::Scheduler),
        ("schedule", Backend::Scheduler),
        ("message", Backend::Channel),
        ("notify", Backend::Notifier),
        ("spawn_agent", Backend::Agents),
    ];

    let refused: Vec<(String, Backend)> = cases
        .iter()
        .map(|(name, _)| {
            let config = ToolsConfig::new([*name]).expect("a name");
            let missing = Tools::builder()
                .build(&config)
                .expect_err("no backend was given");
            (missing.tool().to_owned(), missing.backend())
        })
        .collect();

    let expected: Vec<(String, Backend)> = cases
        .iter()
        .map(|(name, backend)| ((*name).to_owned(), *backend))
        .collect();
    assert_eq!(refused, expected);
}

#[test]
fn a_missing_backend_names_the_tool_and_backend_in_words() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let env = LocalEnv::new(directory.path()).expect("the directory exists");
    let config = ToolsConfig::new(["shell", "exec_command"]).expect("two names");

    let missing = Tools::builder()
        .with_env(&env)
        .build(&config)
        .expect_err("no sessions");

    assert_eq!(
        (missing.tool(), missing.backend()),
        ("exec_command", Backend::Sessions)
    );
    assert_eq!(
        missing.to_string(),
        "exec_command needs a sessions backend, and the builder was given none"
    );
    assert_eq!(Backend::Sessions.to_string(), "sessions");
}

/// A family pattern passes over tools whose backend is missing instead of
/// refusing, so it offers only what the builder can build.
#[test]
fn a_pattern_offers_only_what_the_builder_can_build() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let scheduler = LocalScheduler::open(directory.path()).expect("an empty directory opens");
    let channel = LocalChannel::new(|_line| {});
    let sessions = Sessions::default();

    let none = Tools::builder()
        .build(&ToolsConfig::new(["memory_*", "cron_*"]).expect("two patterns"))
        .expect("a pattern never refuses");
    let given = Tools::builder()
        .with_scheduler(&scheduler)
        .with_channel(&channel)
        .with_sessions(&sessions)
        .build(&ToolsConfig::new(["cron_*", "message", "write_*"]).expect("three entries"))
        .expect("built");

    assert_eq!(none.names().count(), 0);
    assert_eq!(
        given.names().collect::<Vec<_>>(),
        [
            "cron_add",
            "cron_list",
            "cron_update",
            "cron_remove",
            "cron_run",
            "cron_runs",
            "message"
        ],
        "write_stdin needs an env too, so the pattern passes over it"
    );
}

/// A tool added with `with_tool` is offered ahead of a built-in of the same
/// name, and a call reaches it.
#[tokio::test]
async fn an_added_tool_is_offered_ahead_of_a_built_in_of_its_name() {
    let memory = LocalMemory::default();
    let tools = Tools::builder()
        .with_memory(&memory)
        .with_tool(echo("memory_store"))
        .build(&ToolsConfig::new(["memory_store", "memory_list"]).expect("two names"))
        .expect("built");
    let call_ref = guffbolt::CallRef {
        step: 4,
        index: 2,
        call_id: "c".to_owned(),
    };
    let context = Context {
        call_ref: &call_ref,
        run_id: "run-1",
        wake: None,
    };

    let output = tools.dispatch("memory_store", &json!({}), context).await;

    assert_eq!(
        tools.names().collect::<Vec<_>>(),
        ["memory_store", "memory_list"]
    );
    assert_eq!(output.text, "memory_store {} at step 4 call 2");
    assert_eq!(memory.entries(), Vec::new(), "the built-in never ran");
}

#[test]
fn a_config_entry_that_is_no_name_or_pattern_is_refused() {
    let refused: Vec<String> = ["", "memory_**", "*_*"]
        .iter()
        .map(|entry| {
            ToolsConfig::new(["shell", entry])
                .expect_err("not an entry")
                .entry()
                .to_owned()
        })
        .collect();

    assert_eq!(refused, ["", "memory_**", "*_*"]);
    assert_eq!(
        ToolsConfig::new(["a**"])
            .expect_err("two stars")
            .to_string(),
        r#""a**" is neither a tool name nor a pattern with one `*`"#
    );
}

#[test]
fn built_in_names_each_tool_the_builder_builds_and_what_it_needs() {
    let shell = built_in("shell").expect("shell is built in");
    let exec = built_in("exec_command").expect("exec_command is built in");
    let clock = built_in("current_time").expect("current_time is built in");

    assert_eq!((shell.name, shell.needs), ("shell", &[Backend::Env][..]));
    assert_eq!(exec.needs, [Backend::Env, Backend::Sessions]);
    assert!(clock.needs.is_empty(), "{clock:?}");
    assert_eq!(built_in("teleport"), None);
    assert_eq!(built_in(""), None);
}
