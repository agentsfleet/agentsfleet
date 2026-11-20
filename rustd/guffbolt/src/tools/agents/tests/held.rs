//! Which tools the seven are, and which tools a child holds.

use super::*;

#[test]
fn the_subagent_tools_are_guffbolts_and_named_as_codex_names_them() {
    let (agents, _requests) = Agents::new(CAPS);
    let built: Tools<ContextTool<'_>> = tools(&agents);

    let names: Vec<&str> = built.names().collect();
    let refused = Tools::builder()
        .build(&ToolsConfig::new([SPAWN_AGENT]).expect("a name"))
        .expect_err("no children were given");

    assert_eq!(
        names,
        [
            "spawn_agent",
            "send_input",
            "followup_task",
            "wait_agent",
            "list_agents",
            "close_agent",
            "interrupt_agent"
        ]
    );
    assert_eq!(names, TOOLS);
    assert_eq!(
        (refused.tool(), refused.backend()),
        (SPAWN_AGENT, Backend::Agents)
    );
}

/// A child holds the tools its parent named, each one its parent holds; a
/// name its parent lacks refuses the child, and one at the depth cap holds
/// none of the seven.
#[tokio::test]
async fn a_child_holds_only_the_tools_named_and_none_of_the_seven_at_the_depth_cap() {
    let (agents, requests) = Agents::new(Caps { depth: 1, ..CAPS });
    let built = tools(&agents);
    let puppet = Puppet::default()
        .plays(
            ROOT,
            [
                call(
                    SPAWN_AGENT,
                    json!({"message": "x", "tools": ["teleport"], "fork_turns": "none"}),
                ),
                call(SPAWN_AGENT, json!({"message": "y", "fork_turns": "none"})),
                call(WAIT_AGENT, json!({"target": 1})),
                Play::Say("ok"),
            ],
        )
        .plays(agent(1), [Play::Say("y done")]);
    let family = Family {
        agents: agents.clone(),
        puppet,
        tools: &built,
    };

    let (_ending, transcript) = run_root(
        &family,
        requests,
        vec![user("go")],
        &mut DriveState::default(),
    )
    .await;

    assert_eq!(
        results(&transcript).first().map(String::as_str),
        Some("[child_tool_not_held] teleport is not one of this run's tools")
    );
    let child = family.puppet.sent_by(&agent(1));
    assert_eq!(
        child.first().map(|sent| sent.tools.clone()),
        Some(Vec::new()),
        "a child at the depth cap holds none of the seven"
    );
}
