//! Subagents end to end: the seven tools built with `with_agents`, a root
//! that starts and steers children, and the harness driving each child's
//! turns over the embedder's conversation beside the root.

use guffbolt::agents::{
    Agents, CLOSE_AGENT, Caps, FOLLOWUP_TASK, INTERRUPT_AGENT, Kind, LIST_AGENTS, Listed,
    SEND_INPUT, SPAWN_AGENT, TOOLS, WAIT_AGENT,
};
use guffbolt::tools::{ContextTool, ToolsConfig};
use guffbolt::{DriveState, Plain, Tools};
use serde_json::json;

use crate::family::{Family, ROOT, agent, answered, said, user};
use crate::script::{Play, Script, call, results};

/// The caps the cases run under, unless one says otherwise.
const CAPS: Caps = Caps {
    depth: 2,
    open: 2,
    started: 4,
};

/// The seven subagent tools, and nothing else.
fn seven(agents: &Agents) -> Tools<ContextTool<'_>> {
    let config = ToolsConfig::new(TOOLS).expect("seven names");
    Tools::builder()
        .with_agents(agents)
        .build(&config)
        .expect("the run's children are given")
}

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

/// The root's loop ends while one child is idle and another still runs:
/// both end interrupted with it, and neither is polled again.
#[tokio::test]
async fn a_loop_that_ends_ends_its_children() {
    let (agents, requests) = Agents::new(CAPS);
    let tools = seven(&agents);
    let script = Script::default()
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
                call(WAIT_AGENT, json!({"target": 2, "timeout_ms": 0})),
                Play::Say("leaving"),
            ],
        )
        .plays(agent(1), [Play::Say("quick done")])
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

    assert_eq!(answered(&ending), Some("leaving"));
    assert_eq!(
        results(&transcript).get(3).map(String::as_str),
        Some(r#"{"status":"running"}"#),
        "a zero timeout only looks"
    );
    assert_eq!(
        family.agents.list(),
        [
            Listed {
                agent_id: 1,
                status: Kind::Interrupted,
                depth: 1,
                calls: 0
            },
            Listed {
                agent_id: 2,
                status: Kind::Interrupted,
                depth: 1,
                calls: 0
            },
        ],
        "the idle child and the running one both end with the run"
    );
    assert_eq!(
        family.script.sent_by(&agent(2)).len(),
        1,
        "the hung turn was never retried"
    );
}

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

/// A child interrupted mid-call keeps a whole transcript: the call its turn
/// never finished reads as interrupted on its next turn. Closing it closes
/// the grandchild it started, which is still running.
#[tokio::test]
async fn an_interrupted_child_keeps_a_whole_transcript_and_closing_it_closes_its_children() {
    let (agents, requests) = Agents::new(Caps {
        depth: 3,
        open: 3,
        started: 3,
    });
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
        Some(Status::Interrupted),
        "the run ended, and the child with it"
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

/// A follow-up to a running child is read on its next step. Closing a
/// running grandchild ends its turn closed; closing the child then closes
/// every child it started, one closed already staying closed.
#[tokio::test]
async fn a_running_child_reads_a_followup_and_closing_it_closes_its_children() {
    let (agents, requests) = Agents::new(Caps {
        depth: 3,
        open: 3,
        started: 3,
    });
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

/// The root's wait for a quiet set of children is cut short by its budget:
/// the run stops, and its children end with it.
#[tokio::test(start_paused = true)]
async fn a_root_that_waits_for_quiet_children_past_its_budget_stops_and_ends_them() {
    let (agents, requests) = Agents::new(CAPS);
    let tools = seven(&agents);
    let script = Script::default()
        .plays(
            ROOT,
            [call(
                SPAWN_AGENT,
                json!({"message": "spin", "fork_turns": "none"}),
            )],
        )
        .plays(agent(1), [Play::Hang]);
    let family = Family {
        agents: agents.clone(),
        script,
        tools: &tools,
    };
    let mut quieting = Quieting {
        agents: &family.agents,
    };

    let (ending, _transcript) = family
        .run(
            requests,
            vec![user("go")],
            &mut quieting,
            &mut DriveState::default(),
        )
        .await;

    assert!(matches!(ending, guffbolt::Ending::Stopped), "{ending:?}");
    assert_eq!(
        family.agents.list(),
        [Listed {
            agent_id: 1,
            status: Kind::Interrupted,
            depth: 1,
            calls: 0
        }]
    );
}

/// Before its second model call the root waits for its children to be quiet,
/// within a minute's budget.
struct Quieting<'a> {
    agents: &'a Agents,
}

impl<'f, 't> guffbolt::Extension<crate::family::Chat<'f, 't>> for Quieting<'_> {
    fn budget(&self, _root: &crate::family::Chat<'f, 't>) -> guffbolt::Budget {
        guffbolt::Budget::Within(std::time::Duration::from_mins(1))
    }

    async fn before_model(
        &mut self,
        _root: &mut crate::family::Chat<'f, 't>,
        number: u64,
        _state: &DriveState,
    ) -> Option<guffbolt::Ending<crate::script::Failure>> {
        if number == 2 {
            let _never = self.agents.quiet().await;
        }
        None
    }
}
