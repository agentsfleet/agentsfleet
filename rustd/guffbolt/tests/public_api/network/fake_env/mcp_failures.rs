//! An MCP server over stdio that fails, never answers or never starts.

use super::*;

/// A server's error answers the call that asked, as `mcp_failed`; the run
/// goes on.
#[tokio::test]
async fn a_servers_error_fails_the_call_that_asked_as_mcp_failed() {
    let env = Arc::new(FakeEnv::serving(Behaviour {
        failing: vec![
            "tools/call",
            "resources/list",
            "resources/templates/list",
            "resources/read",
        ],
        ..Behaviour::default()
    }));
    let server = started(&env).await;
    let listed = server.tools().await.expect("the server lists its tools");
    let offered: Vec<Box<ContextTool<'_>>> = listed
        .into_iter()
        .map(|tool| Box::new(tool) as Box<ContextTool<'_>>)
        .collect();
    let tools = mcp_tools(&server, offered);

    let outputs = [
        dispatch(&tools, "mcp__docs__stats", json!({})).await,
        dispatch(&tools, "list_mcp_resources", json!({})).await,
        dispatch(&tools, "list_mcp_resource_templates", json!({})).await,
        dispatch(
            &tools,
            "read_mcp_resource",
            json!({"server": "docs", "uri": "file:///x"}),
        )
        .await,
    ];

    for (output, method) in outputs.iter().zip([
        "tools/call",
        "resources/list",
        "resources/templates/list",
        "resources/read",
    ]) {
        assert_eq!(output.error_code, Some(code::MCP_FAILED), "{}", output.text);
        assert!(
            output.text.contains(&format!("{method} broke")),
            "{}",
            output.text
        );
    }
}

/// A server that never answers fails each call once the call's time is up;
/// the clock is paused, so the wait costs nothing.
#[tokio::test(start_paused = true)]
async fn a_server_that_never_answers_fails_each_call_at_its_timeout() {
    let env = Arc::new(FakeEnv::serving(Behaviour {
        silent: vec![
            "tools/call",
            "resources/list",
            "resources/templates/list",
            "resources/read",
        ],
        ..Behaviour::default()
    }));
    let server = started(&env).await;
    let listed = server.tools().await.expect("the server lists its tools");
    let offered: Vec<Box<ContextTool<'_>>> = listed
        .into_iter()
        .map(|tool| Box::new(tool) as Box<ContextTool<'_>>)
        .collect();
    let tools = mcp_tools(&server, offered);

    let called = dispatch(&tools, "mcp__docs__stats", json!({})).await;
    let listed = dispatch(&tools, "list_mcp_resources", json!({})).await;
    let templates = dispatch(&tools, "list_mcp_resource_templates", json!({})).await;
    let read = dispatch(
        &tools,
        "read_mcp_resource",
        json!({"server": "docs", "uri": "file:///x"}),
    )
    .await;

    assert_eq!(called.text, "[mcp_failed] docs did not answer within 120s");
    assert_eq!(listed.text, "[mcp_failed] deadline has elapsed");
    assert_eq!(templates.text, "[mcp_failed] deadline has elapsed");
    assert_eq!(read.text, "[mcp_failed] deadline has elapsed");
}

#[tokio::test(start_paused = true)]
async fn a_server_that_never_lists_its_tools_fails_at_the_timeout() {
    let env = Arc::new(FakeEnv::serving(Behaviour {
        silent: vec!["tools/list"],
        ..Behaviour::default()
    }));
    let server = started(&env).await;

    let refused = server.tools().await.expect_err("no listing came");

    assert_eq!(refused.to_string(), "the server did not answer within 120s");
}

#[tokio::test]
async fn a_server_that_lists_no_tools_on_error_says_why() {
    let env = Arc::new(FakeEnv::serving(Behaviour {
        failing: vec!["tools/list"],
        ..Behaviour::default()
    }));
    let server = started(&env).await;

    let refused = server.tools().await.expect_err("the listing failed");

    let cause = std::error::Error::source(&refused).map(ToString::to_string);
    assert!(
        cause.is_some_and(|cause| cause.contains("tools/list broke")),
        "{refused:?}"
    );
}

/// A server that exits with a request unanswered ends its session: the
/// request fails then, and does not wait out its timeout.
#[tokio::test(start_paused = true)]
async fn a_server_that_exits_mid_request_fails_the_request() {
    let env = Arc::new(FakeEnv::serving(Behaviour {
        exits_on: vec!["tools/list"],
        ..Behaviour::default()
    }));
    let server = started(&env).await;

    let refused = server.tools().await.expect_err("the server is gone");

    assert_ne!(
        refused.to_string(),
        "the server did not answer within 120s",
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_stdio_server_that_will_not_start_or_initialize_is_refused() {
    let unstartable = Arc::new(FakeEnv {
        refuse_spawn: true,
        ..FakeEnv::default()
    });
    let refusing = Arc::new(FakeEnv::serving(Behaviour {
        refuse_initialize: true,
        ..Behaviour::default()
    }));
    let spawn = Spawn::program("docs-mcp");

    let not_started = Server::stdio("docs", Arc::<FakeEnv>::clone(&unstartable), &spawn)
        .await
        .expect_err("no process");
    let not_initialized = Server::stdio("docs", Arc::<FakeEnv>::clone(&refusing), &spawn)
        .await
        .expect_err("refused");

    let started_cause = std::error::Error::source(&not_started).map(ToString::to_string);
    let initialized_cause = std::error::Error::source(&not_initialized).map(ToString::to_string);
    assert_eq!(
        not_started.to_string(),
        "the server's process would not start"
    );
    assert_eq!(started_cause.as_deref(), Some("no such program"));
    assert!(
        initialized_cause.is_some_and(|cause| cause.contains("initialize refused")),
        "{not_initialized:?}"
    );
    assert_eq!(lock(&unstartable.spawned).len(), 1);
}

/// Dropping the last handle to a server ends its process through the
/// environment.
#[tokio::test]
async fn a_dropped_server_is_killed_through_its_environment() {
    let env = Arc::new(FakeEnv::serving(Behaviour::default()));
    let server = started(&env).await;
    let killed = env.killed_note.notified();

    drop(server);
    killed.await;

    assert_eq!(lock(&env.killed).clone(), [0]);
}
