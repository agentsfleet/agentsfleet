//! `fork_turns` starts a child from as much of the root's history as asked.

use super::*;

/// `fork_turns` takes Codex's values: `none` keeps none of the parent's
/// turns, `all` (and its absence) keeps every one, and a number keeps the
/// parent's last that many.
#[tokio::test]
async fn fork_turns_none_all_and_a_number_start_from_that_much_history() {
    let (agents, requests) = Agents::new(Caps { open: 4, ..CAPS });
    let tools = seven(&agents);
    let script = Script::default()
        .plays(
            ROOT,
            [
                Play::Call(vec![
                    (
                        SPAWN_AGENT,
                        json!({"message": "alone", "fork_turns": "none"}),
                    ),
                    (
                        SPAWN_AGENT,
                        json!({"message": "everything", "fork_turns": " All "}),
                    ),
                    (
                        SPAWN_AGENT,
                        json!({"message": "last one", "fork_turns": "1"}),
                    ),
                    (SPAWN_AGENT, json!({"message": "default"})),
                ]),
                call(WAIT_AGENT, json!({"target": 1})),
                call(WAIT_AGENT, json!({"target": 2})),
                call(WAIT_AGENT, json!({"target": 3})),
                call(WAIT_AGENT, json!({"target": 4})),
                Play::Say("forked"),
            ],
        )
        .plays(agent(1), [Play::Say("ok")])
        .plays(agent(2), [Play::Say("ok")])
        .plays(agent(3), [Play::Say("ok")])
        .plays(agent(4), [Play::Say("ok")]);
    let family = Family {
        agents: agents.clone(),
        script,
        tools: &tools,
    };
    let opening = vec![user("first"), said("1"), user("second")];

    let (ending, _transcript) = family
        .run(requests, opening, &mut Plain, &mut DriveState::default())
        .await;

    assert_eq!(answered(&ending), Some("forked"));
    let first = |id| {
        family
            .script
            .sent_by(&agent(id))
            .first()
            .map(|sent| sent.messages.clone())
    };
    assert_eq!(first(1), Some(vec![user("alone")]), "none: the task alone");
    assert_eq!(
        first(2),
        Some(vec![user("first"), said("1"), user("second\n\neverything")]),
        "all: every turn, the task joined to the last input"
    );
    assert_eq!(
        first(3),
        Some(vec![user("second\n\nlast one")]),
        "1: the last turn"
    );
    assert_eq!(
        first(4),
        Some(vec![user("first"), said("1"), user("second\n\ndefault")]),
        "absent is all"
    );
}

/// A `fork_turns` that is neither `none`, `all`, blank nor a whole number of
/// turns from 1 is refused, and no child starts.
#[tokio::test]
async fn fork_turns_that_is_no_value_codex_takes_is_refused() {
    let (agents, requests) = Agents::new(CAPS);
    let tools = seven(&agents);
    let script = Script::default().plays(
        ROOT,
        [
            Play::Call(vec![
                (SPAWN_AGENT, json!({"message": "x", "fork_turns": "some"})),
                (SPAWN_AGENT, json!({"message": "x", "fork_turns": "0"})),
                (SPAWN_AGENT, json!({"message": "x", "fork_turns": "-1"})),
                (SPAWN_AGENT, json!({"message": "x", "fork_turns": "1.5"})),
                (
                    SPAWN_AGENT,
                    json!({"message": "x", "fork_turns": "\u{1F600}"}),
                ),
            ]),
            Play::Say("refused"),
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

    assert_eq!(answered(&ending), Some("refused"));
    let refusal = "[invalid_arguments] fork_turns must be `none`, `all` or a number of turns";
    assert_eq!(results(&transcript), [refusal; 5]);
    assert_eq!(family.agents.list(), Vec::new(), "no child started");
}

/// `fork_turns` read as Codex reads it: trimmed, blank as `all`, either word
/// in any case, or a whole number of turns from 1.
#[test]
fn fork_parse_reads_codexs_values_and_nothing_else() {
    use std::num::NonZeroUsize;

    use guffbolt::agents::Fork;

    let read: Vec<Option<Fork>> = [
        "",
        "  ",
        "all",
        "ALL",
        " none ",
        "None",
        "3",
        " 12 ",
        "0",
        "-2",
        "2.0",
        "two",
        "\u{1F600}",
    ]
    .iter()
    .map(|written| Fork::parse(written))
    .collect();

    let last = |turns| NonZeroUsize::new(turns).map(Fork::Last);
    assert_eq!(
        read,
        [
            Some(Fork::All),
            Some(Fork::All),
            Some(Fork::All),
            Some(Fork::All),
            Some(Fork::None),
            Some(Fork::None),
            last(3),
            last(12),
            None,
            None,
            None,
            None,
            None,
        ]
    );
}
