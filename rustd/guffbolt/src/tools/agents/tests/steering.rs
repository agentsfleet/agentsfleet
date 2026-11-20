//! A follow-up, a close and the run's end, each on an idle child.

use super::*;

#[tokio::test]
async fn a_finished_child_takes_a_followup_task_with_its_transcript() {
    let (agents, requests) = Agents::new(CAPS);
    let built = tools(&agents);
    let puppet = Puppet::default()
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
        puppet,
        tools: &built,
    };

    let opening = vec![user("count")];
    let (ending, transcript) =
        run_root(&family, requests, opening, &mut DriveState::default()).await;

    assert_eq!(answered(&ending), Some("counted"));
    assert_eq!(
        results(&transcript),
        [
            r#"{"agent_id":1}"#,
            r#"{"status":"done","answer":"one"}"#,
            r#"{"accepted":true}"#,
            r#"{"status":"done","answer":"one, two"}"#,
        ]
    );
    let turns = family.puppet.sent_by(&agent(1));
    assert_eq!(turns.len(), 2, "the child ran again");
    assert_eq!(
        turns.get(1).map(|sent| sent.messages.clone()),
        Some(vec![user("count to one"), said("one"), user("now to two")]),
        "its second request holds its first turn"
    );
}

#[tokio::test]
async fn close_agent_ends_an_idle_child_and_frees_its_seat() {
    let (agents, requests) = Agents::new(Caps { open: 1, ..CAPS });
    let built = tools(&agents);
    let puppet = Puppet::default()
        .plays(
            ROOT,
            [
                call(SPAWN_AGENT, json!({"message": "a", "fork_turns": "none"})),
                call(WAIT_AGENT, json!({"target": 1})),
                call(SPAWN_AGENT, json!({"message": "b", "fork_turns": "none"})),
                call(CLOSE_AGENT, json!({"target": 1})),
                call(LIST_AGENTS, json!({})),
                call(SPAWN_AGENT, json!({"message": "c", "fork_turns": "none"})),
                call(WAIT_AGENT, json!({"target": 2})),
                Play::Say("done"),
            ],
        )
        .plays(agent(1), [Play::Say("did a")])
        .plays(agent(2), [Play::Say("did c")]);
    let family = Family {
        agents: agents.clone(),
        puppet,
        tools: &built,
    };

    let (ending, transcript) = run_root(
        &family,
        requests,
        vec![user("go")],
        &mut DriveState::default(),
    )
    .await;

    assert_eq!(answered(&ending), Some("done"));
    assert_eq!(
        results(&transcript),
        [
            r#"{"agent_id":1}"#,
            r#"{"status":"done","answer":"did a"}"#,
            "[child_cap_reached] as many children are open, or were started, as one run may; \
             close one that is no longer needed",
            r#"{"previous_status":"done"}"#,
            r#"[{"agent_id":1,"status":"closed","depth":1,"calls":0}]"#,
            r#"{"agent_id":2}"#,
            r#"{"status":"done","answer":"did c"}"#,
        ],
        "an idle child holds its seat until it is closed"
    );
}

#[tokio::test]
async fn a_run_that_ends_ends_its_idle_children() {
    let (agents, requests) = Agents::new(CAPS);
    let built = tools(&agents);
    let puppet = Puppet::default()
        .plays(
            ROOT,
            [
                call(
                    SPAWN_AGENT,
                    json!({"message": "quick", "fork_turns": "none"}),
                ),
                call(
                    SPAWN_AGENT,
                    json!({"message": "slow", "fork_turns": "none"}),
                ),
                call(WAIT_AGENT, json!({"target": 1})),
                Play::Say("leaving"),
            ],
        )
        .plays(agent(1), [Play::Say("quick done")])
        .plays(agent(2), [Play::Hang]);
    let family = Family {
        agents: agents.clone(),
        puppet,
        tools: &built,
    };

    let (ending, _transcript) = run_root(
        &family,
        requests,
        vec![user("go")],
        &mut DriveState::default(),
    )
    .await;

    assert_eq!(answered(&ending), Some("leaving"));
    let statuses: Vec<Kind> = family
        .agents
        .list()
        .iter()
        .map(|child| child.status)
        .collect();
    assert_eq!(
        statuses,
        [Kind::Done, Kind::Interrupted],
        "the running child ends with the run; the idle one keeps its answer"
    );
}
