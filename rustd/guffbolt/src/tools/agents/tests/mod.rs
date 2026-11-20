//! Subagents end to end: the seven tools built through the builder, a root
//! that starts and steers children, and the harness driving each child's
//! turns over the embedder's conversation (§13 of `M253_002`).

#![expect(
    clippy::expect_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

mod fixture;

use serde_json::json;

use self::fixture::{Family, Play, Puppet, ROOT, Saving, agent, call, results, tools};
use crate::agents::{
    Agents, CLOSE_AGENT, Caps, Children, FOLLOWUP_TASK, Kind, LIST_AGENTS, SPAWN_AGENT, Status,
    TOOLS, WAIT_AGENT,
};
use crate::drive::{DriveState, drive};
use crate::extension::Plain;
use crate::step::Ending;
use crate::tool::Tools;
use crate::tools::{Backend, ContextTool, ToolsConfig};
use crate::transcript::{Message, Replay};

/// The caps the cases run under, unless one says otherwise.
const CAPS: Caps = Caps {
    depth: 2,
    open: 2,
    started: 4,
};

fn user(text: &str) -> Message {
    Message::User(text.to_owned())
}

fn said(text: &str) -> Message {
    Message::Assistant {
        text: text.to_owned(),
        calls: Vec::new(),
        replay: Replay::default(),
    }
}

/// The text of an answered ending.
fn answered(ending: &Ending<fixture::Exhausted>) -> Option<&str> {
    match ending {
        Ending::Answered(text) => Some(text),
        Ending::Failed(_) | Ending::Stopped | Ending::Waiting(_) => None,
    }
}

/// Drives the root of `family` from `state` over `opening`, its children
/// beside it, and ends the children once it ends; answers its ending and its
/// transcript.
async fn run_root(
    family: &Family<'_>,
    requests: crate::agents::Requests,
    opening: Vec<Message>,
    state: &mut DriveState,
) -> (Ending<fixture::Exhausted>, Vec<Message>) {
    let mut root = family.root(opening);
    let mut children = Children::new(requests);
    let ending = children
        .beside(family, drive(&mut root, &mut Plain, state))
        .await;
    let _quiet = children.end_all(&family.agents);
    (ending, root.transcript.messages().to_vec())
}

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
        [Kind::Interrupted, Kind::Interrupted],
        "the idle child and the running one both end with the run"
    );
}

#[tokio::test]
async fn a_resumed_loop_restores_its_children_from_drive_state() {
    let (agents, requests) = Agents::new(CAPS);
    let built = tools(&agents);
    let puppet = Puppet::default()
        .plays(
            ROOT,
            [
                call(
                    SPAWN_AGENT,
                    json!({"message": "count", "fork_turns": "none"}),
                ),
                call(WAIT_AGENT, json!({"target": 1})),
                Play::Say("saved"),
            ],
        )
        .plays(agent(1), [Play::Say("one")]);
    let family = Family {
        agents: agents.clone(),
        puppet,
        tools: &built,
    };
    let mut saving = Saving {
        agents: &family.agents,
        at: 3,
        saved: None,
    };
    let mut root = family.root(vec![user("go")]);
    let mut children = Children::new(requests);
    let first = children
        .beside(
            &family,
            drive(&mut root, &mut saving, &mut DriveState::default()),
        )
        .await;
    let _quiet = children.end_all(&family.agents);
    let saved = saving
        .saved
        .expect("the root saved its children before step 3");
    let encoded = serde_json::to_string(&saved).expect("a state encodes");
    let mut decoded: DriveState = serde_json::from_str(&encoded).expect("and decodes");

    let child = decoded.children().children.first().cloned();
    assert_eq!(answered(&first), Some("saved"));
    assert_eq!(
        child.map(|child| (child.status, child.transcript)),
        Some((
            Status::Done("one".to_owned()),
            vec![user("count"), said("one")]
        )),
        "the saved state carries the child's status and transcript"
    );

    let (restored, requests) = Agents::restored(CAPS, &decoded);
    let rebuilt = tools(&restored);
    let puppet = Puppet::default()
        .plays(
            ROOT,
            [
                call(FOLLOWUP_TASK, json!({"target": 1, "message": "again"})),
                call(WAIT_AGENT, json!({"target": 1})),
                Play::Say("resumed"),
            ],
        )
        .plays(agent(1), [Play::Say("two")]);
    let resumed = Family {
        agents: restored.clone(),
        puppet,
        tools: &rebuilt,
    };
    let listed = resumed.agents.list();
    let (ending, transcript) =
        run_root(&resumed, requests, vec![user("go on")], &mut decoded).await;

    assert_eq!(
        listed.iter().map(|child| child.status).collect::<Vec<_>>(),
        [Kind::Done]
    );
    assert_eq!(answered(&ending), Some("resumed"));
    assert_eq!(
        results(&transcript).last().map(String::as_str),
        Some(r#"{"status":"done","answer":"two"}"#)
    );
    assert_eq!(
        resumed
            .puppet
            .sent_by(&agent(1))
            .first()
            .map(|sent| sent.messages.clone()),
        Some(vec![user("count"), said("one"), user("again")]),
        "the restored child goes on from its saved transcript"
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
