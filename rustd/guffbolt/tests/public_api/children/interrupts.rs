//! An interrupted or closed child keeps a whole transcript and closes its
//! own children.

use super::*;

/// `interrupt_agent` ends a running turn where it is; the child stays open
/// and idle, and its next turn reads what was sent while it ran.
#[tokio::test]
async fn interrupt_agent_ends_a_running_turn_and_the_child_stays_open() {
    let (agents, requests) = Agents::new(CAPS);
    let tools = seven(&agents);
    let script = Script::default()
        .plays(
            ROOT,
            [
                call(
                    SPAWN_AGENT,
                    json!({"message": "spin", "fork_turns": "none"}),
                ),
                call(WAIT_AGENT, json!({"target": 1, "timeout_ms": 0})),
                call(SEND_INPUT, json!({"target": 1, "message": "hurry"})),
                call(INTERRUPT_AGENT, json!({"target": 1})),
                call(WAIT_AGENT, json!({"target": 1})),
                call(
                    FOLLOWUP_TASK,
                    json!({"target": 1, "message": "stop spinning"}),
                ),
                call(WAIT_AGENT, json!({"target": 1})),
                Play::Say("stopped it"),
            ],
        )
        .plays(agent(1), [Play::Hang, Play::Say("stopped")]);
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

    assert_eq!(answered(&ending), Some("stopped it"));
    assert_eq!(
        results(&transcript),
        [
            r#"{"agent_id":1}"#,
            r#"{"status":"running"}"#,
            r#"{"accepted":true}"#,
            r#"{"previous_status":"running"}"#,
            r#"{"status":"interrupted"}"#,
            r#"{"accepted":true}"#,
            r#"{"status":"done","answer":"stopped"}"#,
        ]
    );
}

/// A child interrupted mid-call keeps a whole transcript: the call its turn
/// never finished reads as interrupted on its next turn. Closing it closes
/// the grandchild it started, which is still running.
#[tokio::test]
async fn an_interrupted_child_keeps_a_whole_transcript_and_closing_it_closes_its_children() {
    let (agents, requests) = Agents::new(Caps { depth: 3, open: 3 });
    let tools = seven(&agents);
    let script = Script::default()
        .plays(
            ROOT,
            [
                call(
                    SPAWN_AGENT,
                    json!({"message": "delegate", "fork_turns": "none"}),
                ),
                call(WAIT_AGENT, json!({"target": 1, "timeout_ms": 0})),
                call(INTERRUPT_AGENT, json!({"target": 1})),
                call(WAIT_AGENT, json!({"target": 1})),
                call(FOLLOWUP_TASK, json!({"target": 1, "message": "go on"})),
                call(WAIT_AGENT, json!({"target": 1})),
                call(CLOSE_AGENT, json!({"target": 1})),
                call(WAIT_AGENT, json!({"target": 2})),
                Play::Say("tidied up"),
            ],
        )
        .plays(
            agent(1),
            [
                call(
                    SPAWN_AGENT,
                    json!({"message": "spin", "fork_turns": "none", "tools": ["list_agents"]}),
                ),
                call(WAIT_AGENT, json!({"target": 2, "timeout_ms": 600_000})),
                Play::Say("went on"),
            ],
        )
        .plays(agent(2), [Play::Hang]);
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

    assert_eq!(answered(&ending), Some("tidied up"));
    assert_eq!(
        results(&transcript),
        [
            r#"{"agent_id":1}"#,
            r#"{"status":"running"}"#,
            r#"{"previous_status":"running"}"#,
            r#"{"status":"interrupted"}"#,
            r#"{"accepted":true}"#,
            r#"{"status":"done","answer":"went on"}"#,
            r#"{"previous_status":"done"}"#,
            r#"{"status":"closed"}"#,
        ]
    );
    let second = family.script.sent_by(&agent(1));
    assert_eq!(
        second.get(2).map(|sent| results(&sent.messages)),
        Some(vec![
            r#"{"agent_id":2}"#.to_owned(),
            "[interrupted] the turn was interrupted before this call finished".to_owned(),
        ]),
        "its next turn reads the unfinished call as interrupted"
    );
    assert_eq!(
        family
            .script
            .sent_by(&agent(2))
            .first()
            .map(|sent| sent.tools.clone()),
        Some(vec!["list_agents".to_owned()]),
        "a child names the subset of its parent's tools it holds"
    );
}

/// A follow-up to a running child is read on its next step. Closing a
/// running grandchild ends its turn closed; closing the child then closes
/// every child it started, one closed already staying closed.
#[tokio::test]
async fn a_running_child_reads_a_followup_and_closing_it_closes_its_children() {
    let (agents, requests) = Agents::new(Caps { depth: 3, open: 3 });
    let tools = seven(&agents);
    let script = Script::default()
        .plays(
            ROOT,
            [
                call(
                    SPAWN_AGENT,
                    json!({"message": "lead", "fork_turns": "none"}),
                ),
                call(WAIT_AGENT, json!({"target": 1, "timeout_ms": 0})),
                call(FOLLOWUP_TASK, json!({"target": 1, "message": "and hurry"})),
                call(CLOSE_AGENT, json!({"target": 2})),
                call(WAIT_AGENT, json!({"target": 1})),
                call(CLOSE_AGENT, json!({"target": 1})),
                call(LIST_AGENTS, json!({})),
                Play::Say("closed both"),
            ],
        )
        .plays(
            agent(1),
            [
                call(
                    SPAWN_AGENT,
                    json!({"message": "help", "fork_turns": "none"}),
                ),
                call(WAIT_AGENT, json!({"target": 2})),
                Play::Say("led"),
            ],
        )
        .plays(agent(2), [Play::Hang]);
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

    assert_eq!(answered(&ending), Some("closed both"));
    assert_eq!(
        results(&transcript),
        [
            r#"{"agent_id":1}"#,
            r#"{"status":"running"}"#,
            r#"{"accepted":true}"#,
            r#"{"previous_status":"running"}"#,
            r#"{"status":"done","answer":"led"}"#,
            r#"{"previous_status":"done"}"#,
            r#"[{"agent_id":1,"status":"closed","depth":1,"calls":2},{"agent_id":2,"status":"closed","depth":2,"calls":0}]"#,
        ]
    );
    let last = family
        .script
        .sent_by(&agent(1))
        .last()
        .map(|sent| sent.messages.clone());
    assert_eq!(
        last.and_then(|messages| messages.last().cloned()),
        Some(user("and hurry")),
        "the follow-up reached the running child on its next step"
    );
}

/// An interrupt and a follow-up in one step: the follow-up reaches the child
/// while its interrupted turn still unwinds, and that turn ends without
/// reading it, so the follow-up runs a turn of its own rather than being
/// lost.
#[tokio::test]
async fn a_followup_beside_an_interrupt_runs_its_own_turn() {
    let (agents, requests) = Agents::new(CAPS);
    let tools = seven(&agents);
    let script = Script::default()
        .plays(
            ROOT,
            [
                call(
                    SPAWN_AGENT,
                    json!({"message": "spin", "fork_turns": "none"}),
                ),
                call(WAIT_AGENT, json!({"target": 1, "timeout_ms": 0})),
                Play::Call(vec![
                    (INTERRUPT_AGENT, json!({"target": 1})),
                    (FOLLOWUP_TASK, json!({"target": 1, "message": "go on"})),
                ]),
                call(WAIT_AGENT, json!({"target": 1})),
                Play::Say("went on"),
            ],
        )
        .plays(agent(1), [Play::Hang, Play::Say("went on")]);
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

    assert_eq!(answered(&ending), Some("went on"));
    assert_eq!(
        results(&transcript),
        [
            r#"{"agent_id":1}"#,
            r#"{"status":"running"}"#,
            r#"{"previous_status":"running"}"#,
            r#"{"accepted":true}"#,
            r#"{"status":"done","answer":"went on"}"#,
        ]
    );
    let second = family.script.sent_by(&agent(1));
    assert_eq!(
        second.get(1).and_then(|sent| sent.messages.last().cloned()),
        Some(user("spin\n\ngo on")),
        "the follow-up's turn read it, joined to the task it never answered"
    );
}
