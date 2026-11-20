//! The caps on open and started children, and the registry that holds them.

use super::*;

/// An idle child holds its seat until `close_agent` frees it; the run's
/// `started` cap holds even once seats are free.
#[tokio::test]
async fn close_agent_frees_a_seat_under_the_open_cap_and_started_still_binds() {
    let (agents, requests) = Agents::new(Caps {
        depth: 2,
        open: 1,
        started: 2,
    });
    let tools = seven(&agents);
    let capped = "[child_cap_reached] as many children are open, or were started, as one run \
                  may; close one that is no longer needed";
    let script = Script::default()
        .plays(
            ROOT,
            [
                call(SPAWN_AGENT, json!({"message": "a", "fork_turns": "none"})),
                call(WAIT_AGENT, json!({"target": 1})),
                call(SPAWN_AGENT, json!({"message": "b", "fork_turns": "none"})),
                call(CLOSE_AGENT, json!({"target": 1})),
                call(SPAWN_AGENT, json!({"message": "c", "fork_turns": "none"})),
                call(WAIT_AGENT, json!({"target": 2})),
                call(CLOSE_AGENT, json!({"target": 2})),
                call(SPAWN_AGENT, json!({"message": "d", "fork_turns": "none"})),
                Play::Say("done"),
            ],
        )
        .plays(agent(1), [Play::Say("did a")])
        .plays(agent(2), [Play::Say("did c")]);
    let family = Family {
        agents: agents.clone(),
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

    assert_eq!(answered(&ending), Some("done"));
    assert_eq!(
        results(&transcript),
        [
            r#"{"agent_id":1}"#,
            r#"{"status":"done","answer":"did a"}"#,
            capped,
            r#"{"previous_status":"done"}"#,
            r#"{"agent_id":2}"#,
            r#"{"status":"done","answer":"did c"}"#,
            r#"{"previous_status":"done"}"#,
            capped,
        ]
    );
}

/// A child asks for a tool its parent does not hold: refused, and no child
/// starts. A child at the depth cap holds none of the seven.
#[tokio::test]
async fn a_child_holds_only_its_parents_tools_and_none_of_the_seven_at_the_depth_cap() {
    let (agents, requests) = Agents::new(Caps { depth: 1, ..CAPS });
    let tools = seven(&agents);
    let script = Script::default()
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
        .plays(
            agent(1),
            [
                call(SPAWN_AGENT, json!({"message": "z"})),
                Play::Say("y done"),
            ],
        );
    let family = Family {
        agents: agents.clone(),
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

    assert_eq!(answered(&ending), Some("ok"));
    assert_eq!(
        results(&transcript).first().map(String::as_str),
        Some("[child_tool_not_held] teleport is not one of this run's tools")
    );
    let child = family.script.sent_by(&agent(1));
    assert_eq!(
        child.first().map(|sent| sent.tools.clone()),
        Some(Vec::new())
    );
    assert_eq!(
        child.get(1).map(|sent| results(&sent.messages)),
        Some(vec![
            "[tool_not_offered] spawn_agent is not one of this run's tools".to_owned()
        ]),
        "a call to a tool the child does not hold is refused, not run"
    );
    assert_eq!(family.agents.list().len(), 1, "only y started");
}

/// What an embedder reads of a run's children outside the tools: a wait
/// with no timeout, statuses by name, and the registry's own view of itself.
#[tokio::test]
async fn the_registry_reads_back_through_its_own_methods() {
    use guffbolt::agents::{Action, Children, Order, Status};

    let (agents, requests) = Agents::new(CAPS);
    let (other, _other_requests) = Agents::new(CAPS);
    let tools = seven(&agents);
    let script = Script::default()
        .plays(
            ROOT,
            [
                call(SPAWN_AGENT, json!({"message": "x", "fork_turns": "none"})),
                call(WAIT_AGENT, json!({"target": 1})),
                Play::Say("ok"),
            ],
        )
        .plays(agent(1), [Play::Say("done x")]);
    let family = Family {
        agents: agents.clone(),
        script,
        tools: &tools,
    };
    let children = Children::new(Agents::new(CAPS).1);

    let (_ending, _transcript) = family
        .run(
            requests,
            vec![user("go")],
            &mut Plain,
            &mut DriveState::default(),
        )
        .await;
    let waited = agents.wait(1, None).await;
    let missing = agents.wait(5, None).await;

    assert_eq!(
        waited,
        Some(Status::Done("done x".to_owned())),
        "the run ended, and the idle child kept its answer"
    );
    assert_eq!(missing, None);
    assert!(Status::Done("x".to_owned()).is_idle());
    assert!(!Status::Running.is_idle() && !Status::Closed.is_idle());
    assert_eq!(Status::Failed("x".to_owned()).kind().to_string(), "failed");
    assert_eq!(
        format!("{agents:?}"),
        "Agents { caps: Caps { depth: 2, open: 2, started: 4 }, open: 1, running: 0, started: 1, .. }"
    );
    assert_eq!(
        format!("{children:?}"),
        "Children { running: 0, queued: false, waiting: 0 }"
    );
    assert_eq!(
        Order::new(agents.clone(), Action::List),
        Order::new(agents.clone(), Action::List)
    );
    assert_ne!(
        Order::new(agents.clone(), Action::List),
        Order::new(other, Action::List)
    );
    assert_ne!(
        Order::new(agents.clone(), Action::List),
        Order::new(agents.clone(), Action::Close { target: 1 })
    );
}
