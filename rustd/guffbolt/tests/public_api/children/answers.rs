//! A child answers, fails, or answers nothing, and the root reads which.

use super::*;

#[tokio::test]
async fn a_spawned_child_answers_and_wait_agent_reads_its_answer() {
    let (agents, requests) = Agents::new(CAPS);
    let tools = seven(&agents);
    let script = Script::default()
        .plays(
            ROOT,
            [
                call(
                    SPAWN_AGENT,
                    json!({"message": "check the build", "fork_turns": "none"}),
                ),
                call(WAIT_AGENT, json!({"target": 1})),
                call(LIST_AGENTS, json!({})),
                Play::Say("the build is green"),
            ],
        )
        .plays(agent(1), [Play::Say("green")]);
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

    assert_eq!(answered(&ending), Some("the build is green"));
    assert_eq!(
        results(&transcript),
        [
            r#"{"agent_id":1}"#,
            r#"{"status":"done","answer":"green"}"#,
            r#"[{"agent_id":1,"status":"done","depth":1,"calls":0}]"#,
        ]
    );
    let child = family.script.sent_by(&agent(1));
    assert_eq!(
        child.first().map(|sent| sent.messages.clone()),
        Some(vec![user("check the build")]),
        "the child reads its task alone"
    );
    assert_eq!(
        child.first().map(|sent| sent.tools.clone()),
        Some(TOOLS.map(str::to_owned).to_vec()),
        "a child under the depth cap holds its parent's tools"
    );
}

/// A child whose own model fails is idle with the failure's detail, and the
/// root's run goes on.
#[tokio::test]
async fn a_child_whose_model_fails_reads_failed_with_its_detail() {
    let (agents, requests) = Agents::new(CAPS);
    let tools = seven(&agents);
    let script = Script::default().plays(
        ROOT,
        [
            call(SPAWN_AGENT, json!({"message": "x", "fork_turns": "none"})),
            call(WAIT_AGENT, json!({"target": 1})),
            Play::Say("it failed"),
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

    assert_eq!(answered(&ending), Some("it failed"));
    assert_eq!(
        results(&transcript).get(1).map(String::as_str),
        Some(r#"{"status":"failed","detail":"the script ran out"}"#)
    );
}

/// A child whose model answers nothing fails with the embedder's own
/// failure for a silent step.
#[tokio::test]
async fn a_child_whose_model_answers_nothing_fails_as_the_embedder_says() {
    let (agents, requests) = Agents::new(CAPS);
    let tools = seven(&agents);
    let script = Script::default()
        .plays(
            ROOT,
            [
                call(SPAWN_AGENT, json!({"message": "x", "fork_turns": "none"})),
                call(WAIT_AGENT, json!({"target": 1})),
                Play::Say("it said nothing"),
            ],
        )
        .plays(agent(1), [Play::Say("")]);
    let family = Family {
        agents: agents.clone(),
        script,
        tools: &tools,
    };

    let (_ending, transcript) = family
        .run(
            requests,
            vec![user("go")],
            &mut Plain,
            &mut DriveState::default(),
        )
        .await;

    assert_eq!(
        results(&transcript).get(1).map(String::as_str),
        Some(r#"{"status":"failed","detail":"the model answered nothing (cut: false)"}"#)
    );
}
