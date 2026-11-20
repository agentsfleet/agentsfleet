//! A named tool whose backend was not given refuses the build, and says which.

use super::*;

#[test]
fn a_tool_without_its_backend_refuses_to_start() {
    let refused = Tools::builder()
        .build(&config(&["cron_add"]))
        .expect_err("no scheduler was given");

    assert_eq!(refused.tool(), "cron_add");
    assert_eq!(refused.backend(), Backend::Scheduler);
    assert_eq!(
        refused.to_string(),
        "cron_add needs a scheduler backend, and the builder was given none"
    );
}

/// Every family's tool, named without its backend, names that backend.
#[test]
fn each_backend_a_named_tool_lacks_is_named() {
    let fake = Fake::answering(Ok(String::new()));
    let cases = [
        ("file_read", Backend::Env),
        ("shell", Backend::Env),
        ("web_fetch", Backend::Outbound),
        ("memory_store", Backend::Memory),
        ("schedule", Backend::Scheduler),
        ("message", Backend::Channel),
        ("notify", Backend::Notifier),
        ("spawn_agent", Backend::Agents),
        ("exec_command", Backend::Env),
    ];

    for (tool, backend) in cases {
        let refused = Tools::builder()
            .with_tool(ServerTool::named("unrelated".to_owned()))
            .build(&config(&["unrelated", "update_plan", tool]))
            .expect_err("the named tool's backend is missing");
        assert_eq!((refused.tool(), refused.backend()), (tool, backend));
    }
    let given = Tools::builder()
        .with_scheduler(&fake)
        .with_channel(&fake)
        .with_notifier(&fake)
        .build(&config(&["schedule", "message", "notify"]))
        .expect("every named backend is given");
    assert_eq!(names(&given), ["schedule", "message", "notify"]);
}

/// A family pattern offers what the builder can build and passes over the
/// rest; a name nothing holds offers nothing.
#[test]
fn a_pattern_passes_over_a_tool_whose_backend_is_missing() {
    let tools = Tools::builder()
        .build(&config(&["cron_*", "*_time", "teleport"]))
        .expect("a pattern refuses nothing");

    assert_eq!(names(&tools), ["current_time"]);
}

#[test]
fn an_entry_with_no_name_or_two_stars_is_refused() {
    for entry in ["", "mcp__*__*", "**"] {
        let refused = ToolsConfig::new([entry]).expect_err("not a name or one pattern");
        assert_eq!(refused.entry(), entry);
        assert!(refused.to_string().contains("one `*`"), "{refused}");
    }
    assert_eq!(
        ToolsConfig::new(["*", "mcp__docs__*", "memory_store"]).map(|_ok| ()),
        Ok(())
    );
}

/// `exec_command` and `write_stdin` keep their processes in the run's
/// sessions, so an environment alone does not build them.
#[test]
fn a_session_tool_names_the_sessions_it_lacks() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let local = local_at(root.path());

    for tool in ["exec_command", "write_stdin"] {
        let refused = Tools::builder()
            .with_env(&local.env)
            .build(&config(&[tool]))
            .expect_err("no sessions were given");
        assert_eq!(
            (refused.tool(), refused.backend()),
            (tool, Backend::Sessions)
        );
    }
    let built = local
        .builder()
        .build(&config(&["exec_command", "write_stdin"]))
        .expect("the environment and sessions are given");
    assert_eq!(names(&built), ["exec_command", "write_stdin"]);
}

/// A name no built-in has builds nothing, even past every family the chain
/// asks, down to the agent tools'.
#[test]
fn a_name_no_built_in_has_builds_nothing() {
    let caps = crate::agents::Caps {
        depth: 1,
        open: 1,
        started: 1,
    };
    let (agents, _requests) = crate::agents::Agents::new(caps);
    let given = super::super::Given {
        agents: Some(&agents),
        ..super::super::Given::default()
    };

    assert!(super::super::catalog::tool(given, "no_such_tool").is_none());
}
