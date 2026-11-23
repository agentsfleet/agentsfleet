//! Follow-ups and steering reach an open child, and nothing reaches a closed
//! or unknown one.

use super::*;

#[tokio::test]
async fn followup_task_runs_an_idle_child_again_over_its_transcript() {
    let (agents, requests) = Agents::new(CAPS);
    let tools = seven(&agents);
    let script = Script::default()
        .plays(
            ROOT,
            [
                call(
                    SPAWN_AGENT,
                    json!({"message": "count to one", "fork_turns": "none"}),
                ),
                call(WAIT_AGENT, json!({"target": 1})),
                call(FOLLOWUP_TASK, json!({"target": 1, "message": "now to two"})),
                call(WAIT_AGENT, json!({"target": 1})),
                Play::Say("counted"),
            ],
        )
        .plays(agent(1), [Play::Say("one"), Play::Say("one, two")]);
    let family = Family {
        agents: agents.clone(),
        script,
        tools: &tools,
    };

    let (ending, transcript) = family
        .run(
            requests,
            vec![user("count")],
            &mut Plain,
            &mut DriveState::default(),
        )
        .await;

    assert_eq!(answered(&ending), Some("counted"));
    let turns = family.script.sent_by(&agent(1));
    assert_eq!(
        turns.get(1).map(|sent| sent.messages.clone()),
        Some(vec![user("count to one"), said("one"), user("now to two")]),
        "its second turn holds its first"
    );
    assert_eq!(
        results(&transcript),
        [
            r#"{"agent_id":1}"#,
            r#"{"status":"done","answer":"one"}"#,
            r#"{"accepted":true}"#,
            r#"{"status":"done","answer":"one, two"}"#,
        ],
        "the second wait reads the second turn's answer"
    );
}

/// Every steering call that names no child of the run reads the same
/// refusal, and no child starts.
#[tokio::test]
async fn steering_a_child_the_run_does_not_have_is_refused() {
    let (agents, requests) = Agents::new(CAPS);
    let tools = seven(&agents);
    let script = Script::default().plays(
        ROOT,
        [
            Play::Call(vec![
                (FOLLOWUP_TASK, json!({"target": 9, "message": "x"})),
                (SEND_INPUT, json!({"target": 9, "message": "x"})),
                (WAIT_AGENT, json!({"target": 9, "timeout_ms": 0})),
                (CLOSE_AGENT, json!({"target": 9})),
                (INTERRUPT_AGENT, json!({"target": 9})),
            ]),
            Play::Say("nobody there"),
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

    assert_eq!(answered(&ending), Some("nobody there"));
    let refusal = "[child_not_found] no child of this run has id 9";
    assert_eq!(results(&transcript), [refusal; 5]);
    assert_eq!(family.agents.list(), Vec::new());
}

/// A closed child takes nothing more: no message and no follow-up.
#[tokio::test]
async fn a_closed_child_takes_no_message_and_no_followup() {
    let (agents, requests) = Agents::new(CAPS);
    let tools = seven(&agents);
    let script = Script::default()
        .plays(
            ROOT,
            [
                call(SPAWN_AGENT, json!({"message": "a", "fork_turns": "none"})),
                call(WAIT_AGENT, json!({"target": 1})),
                call(CLOSE_AGENT, json!({"target": 1})),
                Play::Call(vec![
                    (SEND_INPUT, json!({"target": 1, "message": "more"})),
                    (FOLLOWUP_TASK, json!({"target": 1, "message": "again"})),
                    (WAIT_AGENT, json!({"target": 1, "timeout_ms": 0})),
                ]),
                Play::Say("closed"),
            ],
        )
        .plays(agent(1), [Play::Say("did a")]);
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

    assert_eq!(answered(&ending), Some("closed"));
    assert_eq!(
        results(&transcript),
        [
            r#"{"agent_id":1}"#,
            r#"{"status":"done","answer":"did a"}"#,
            r#"{"previous_status":"done"}"#,
            r#"{"accepted":false}"#,
            r#"{"accepted":false}"#,
            r#"{"status":"closed"}"#,
        ]
    );
    assert_eq!(
        family.script.sent_by(&agent(1)).len(),
        1,
        "the closed child never ran again"
    );
}
