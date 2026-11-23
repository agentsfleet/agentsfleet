//! What the builder builds, refuses and offers from a config.

use super::*;

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
