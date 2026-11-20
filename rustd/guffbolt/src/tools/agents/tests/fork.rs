//! `spawn_agent` with `fork_turns`: the parent's last turns, or a refusal.

use super::*;

/// `fork_turns` takes Codex's values: a number keeps the parent's last that
/// many turns, `all`, the default, keeps every turn, and `none` keeps none.
#[tokio::test]
async fn spawn_agent_with_fork_turns_starts_from_the_parents_last_turns() {
    let (agents, requests) = Agents::new(Caps { open: 3, ..CAPS });
    let built = tools(&agents);
    let puppet = Puppet::default()
        .plays(
            ROOT,
            [
                Play::Call(vec![
                    (
                        SPAWN_AGENT,
                        json!({"message": "last two", "fork_turns": "2"}),
                    ),
                    (SPAWN_AGENT, json!({"message": "everything"})),
                    (
                        SPAWN_AGENT,
                        json!({"message": "alone", "fork_turns": "None"}),
                    ),
                ]),
                call(WAIT_AGENT, json!({"target": 1})),
                call(WAIT_AGENT, json!({"target": 2})),
                call(WAIT_AGENT, json!({"target": 3})),
                Play::Say("checked"),
            ],
        )
        .plays(agent(1), [Play::Say("fine")])
        .plays(agent(2), [Play::Say("fine")])
        .plays(agent(3), [Play::Say("fine")]);
    let family = Family {
        agents: agents.clone(),
        puppet,
        tools: &built,
    };
    let opening = vec![
        user("first"),
        said("1"),
        user("second"),
        said("2"),
        user("third"),
    ];

    let (ending, _transcript) =
        run_root(&family, requests, opening, &mut DriveState::default()).await;

    assert_eq!(answered(&ending), Some("checked"));
    let first = |id| {
        family
            .puppet
            .sent_by(&agent(id))
            .first()
            .map(|sent| sent.messages.clone())
    };
    assert_eq!(
        first(1),
        Some(vec![user("second"), said("2"), user("third\n\nlast two")]),
        "the parent's last two turns, none before, then the task"
    );
    assert_eq!(
        first(2),
        Some(vec![
            user("first"),
            said("1"),
            user("second"),
            said("2"),
            user("third\n\neverything"),
        ]),
        "every turn when fork_turns is absent"
    );
    assert_eq!(first(3), Some(vec![user("alone")]), "the task alone");
}

/// A `fork_turns` that is neither `none`, `all` nor a number of turns is
/// refused, and no child starts.
#[tokio::test]
async fn spawn_agent_refuses_a_fork_turns_it_cannot_read() {
    let (agents, requests) = Agents::new(CAPS);
    let built = tools(&agents);
    let puppet = Puppet::default().plays(
        ROOT,
        [
            Play::Call(vec![
                (SPAWN_AGENT, json!({"message": "x", "fork_turns": "some"})),
                (SPAWN_AGENT, json!({"message": "x", "fork_turns": "0"})),
            ]),
            Play::Say("refused"),
        ],
    );
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

    assert_eq!(answered(&ending), Some("refused"));
    let refusal = "[invalid_arguments] fork_turns must be `none`, `all` or a number of turns";
    assert_eq!(results(&transcript), [refusal, refusal]);
    assert_eq!(family.agents.list(), Vec::new(), "no child started");
}
