//! Saving a run mid-way and resuming it: the root's `DriveState` carries
//! every child, its transcript with it, through JSON to a new `Agents`.

use guffbolt::agents::{
    Agents, Caps, FOLLOWUP_TASK, Kind, Listed, SPAWN_AGENT, Status, TOOLS, WAIT_AGENT,
};
use guffbolt::tools::{ContextTool, ToolsConfig};
use guffbolt::{DriveState, Ending, Extension, Plain, Tools};
use serde_json::json;

use crate::family::{Chat, Family, ROOT, agent, answered, said, user};
use crate::script::{Failure, Play, Script, call, results};

const CAPS: Caps = Caps {
    depth: 2,
    open: 2,
    started: 4,
};

fn seven(agents: &Agents) -> Tools<ContextTool<'_>> {
    let config = ToolsConfig::new(TOOLS).expect("seven names");
    Tools::builder()
        .with_agents(agents)
        .build(&config)
        .expect("the run's children are given")
}

/// The root's checkpoint: before model call `at` it asks for a pause and,
/// granted one, saves the children into a copy of its state. Every ask's
/// outcome is kept, by call number.
struct Saving<'a> {
    agents: &'a Agents,
    at: Vec<u64>,
    asked: Vec<(u64, bool)>,
    saved: Option<DriveState>,
}

impl<'f, 't> Extension<Chat<'f, 't>> for Saving<'_> {
    async fn before_model(
        &mut self,
        _root: &mut Chat<'f, 't>,
        number: u64,
        state: &DriveState,
    ) -> Option<Ending<Failure>> {
        if !self.at.contains(&number) {
            return None;
        }
        let pause = self.agents.pause().await;
        self.asked.push((number, pause.is_some()));
        let pause = pause?;
        let mut saved = state.clone();
        if self.agents.save(&pause, &mut saved) {
            self.saved = Some(saved);
        }
        None
    }
}

/// A run that starts a child, waits for its answer and saves before its
/// third model call, its state through JSON and back: what an embedder
/// keeps between processes.
async fn saved_mid_run() -> DriveState {
    let (agents, requests) = Agents::new(CAPS);
    let tools = seven(&agents);
    let script = Script::default()
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
        script,
        tools: &tools,
    };
    let mut saving = Saving {
        agents: &family.agents,
        at: vec![3],
        asked: Vec::new(),
        saved: None,
    };

    let (first, _transcript) = family
        .run(
            requests,
            vec![user("go")],
            &mut saving,
            &mut DriveState::default(),
        )
        .await;

    assert_eq!(answered(&first), Some("saved"));
    assert_eq!(
        saving.asked,
        [(3, true)],
        "granted once the child's turn ended"
    );
    let saved = saving
        .saved
        .expect("the root saved its children before call 3");
    let encoded = serde_json::to_string(&saved).expect("a state encodes");
    let decoded: DriveState = serde_json::from_str(&encoded).expect("and decodes");
    let census = decoded.children();
    assert_eq!((census.next, census.started), (1, 1));
    assert_eq!(
        census
            .children
            .first()
            .map(|child| (child.status.clone(), child.transcript.clone())),
        Some((
            Status::Done("one".to_owned()),
            vec![user("count"), said("one")]
        )),
        "the saved state carries the child's status and transcript"
    );
    assert_eq!(decoded.completed_steps(), 2, "the spawn and the wait");
    decoded
}

#[tokio::test]
async fn a_state_saved_mid_run_resumes_with_its_children_restored() {
    let mut decoded = saved_mid_run().await;

    let (restored, requests) = Agents::restored(CAPS, &decoded);
    let rebuilt = seven(&restored);
    let script = Script::default()
        .plays(
            ROOT,
            [
                call(FOLLOWUP_TASK, json!({"target": 1, "message": "again"})),
                call(WAIT_AGENT, json!({"target": 1})),
                call(
                    SPAWN_AGENT,
                    json!({"message": "fresh", "fork_turns": "none"}),
                ),
                call(WAIT_AGENT, json!({"target": 2})),
                Play::Say("resumed"),
            ],
        )
        .plays(agent(1), [Play::Say("two")])
        .plays(agent(2), [Play::Say("new")]);
    let resumed = Family {
        agents: restored.clone(),
        script,
        tools: &rebuilt,
    };
    assert_eq!(
        resumed.agents.list(),
        [Listed {
            agent_id: 1,
            status: Kind::Done,
            depth: 1,
            calls: 0
        }],
        "the child is back, idle"
    );

    let (ending, transcript) = resumed
        .run(requests, vec![user("go on")], &mut Plain, &mut decoded)
        .await;

    assert_eq!(answered(&ending), Some("resumed"));
    assert_eq!(
        resumed
            .script
            .sent_by(&agent(1))
            .first()
            .map(|sent| sent.messages.clone()),
        Some(vec![user("count"), said("one"), user("again")]),
        "the restored child goes on from its saved transcript"
    );
    assert_eq!(
        results(&transcript).get(2).map(String::as_str),
        Some(r#"{"agent_id":2}"#),
        "ids go on from the saved count"
    );
    assert_eq!(
        decoded.completed_steps(),
        7,
        "the resumed loop's steps add to the saved ones"
    );
    assert_eq!(
        results(&transcript).get(1).map(String::as_str),
        Some(r#"{"status":"done","answer":"two"}"#),
        "the follow-up's wait reads the follow-up's answer"
    );
}

/// A child saved running, which no pause allows, comes back interrupted; a
/// closed one comes back closed and holds no seat.
#[test]
fn a_child_saved_running_is_restored_interrupted_and_a_closed_one_holds_no_seat() {
    let state: DriveState = serde_json::from_value(json!({
        "answers": [],
        "completed_steps": 4,
        "children": {
            "next": 2,
            "started": 2,
            "children": [
                {"id": 1, "parent": 0, "depth": 1, "calls": 3, "tools": [],
                 "status": "Running", "transcript": [], "state": {"answers": [], "completed_steps": 1}},
                {"id": 2, "parent": 0, "depth": 1, "calls": 0, "tools": [],
                 "status": "Closed", "transcript": [], "state": {"answers": [], "completed_steps": 0}}
            ]
        }
    }))
    .expect("a saved state with two children");
    let mut state = state;
    // What the registry restores is what the census holds, rewritten or not.
    if let Some(child) = state.children_mut().children.first_mut() {
        child.calls = 5;
    }

    let (agents, _requests) = Agents::restored(
        Caps {
            depth: 2,
            open: 1,
            started: 4,
        },
        &state,
    );

    assert_eq!(
        agents.list(),
        [
            Listed {
                agent_id: 1,
                status: Kind::Interrupted,
                depth: 1,
                calls: 5
            },
            Listed {
                agent_id: 2,
                status: Kind::Closed,
                depth: 1,
                calls: 0
            },
        ]
    );
    assert_eq!(
        agents.follow_up(2, "again".to_owned()),
        Some(false),
        "closed takes nothing"
    );
    assert_eq!(
        agents.follow_up(7, "again".to_owned()),
        None,
        "no such child"
    );
}

#[test]
fn a_state_with_a_field_it_does_not_know_is_refused() {
    let refused = serde_json::from_value::<DriveState>(json!({
        "answers": [], "completed_steps": 0, "turns": 3
    }))
    .expect_err("an unknown field");

    assert!(
        refused.to_string().starts_with("unknown field `turns`"),
        "{refused}"
    );
}

/// A pause proves one run's children quiet; another run's save refuses it.
#[tokio::test]
async fn a_pause_from_another_run_saves_nothing() {
    let (mine, my_requests) = Agents::new(CAPS);
    let (theirs, _their_requests) = Agents::new(CAPS);
    let mut children = guffbolt::agents::Children::new(my_requests);
    let pause = children
        .end_all(&mine)
        .expect("a run with no turn pauses at once");
    let mut state = DriveState::default();

    let saved_by_theirs = theirs.save(&pause, &mut state);
    let census_by_theirs = theirs.census(&pause);
    let saved_by_mine = mine.save(&pause, &mut state);

    assert!(!saved_by_theirs);
    assert_eq!(census_by_theirs, None);
    assert!(saved_by_mine);
    assert!(
        state.children().is_empty(),
        "a run that started no child saves an empty census"
    );
}

/// A pause is refused while a child's turn runs, so a save never catches a
/// turn half done.
#[tokio::test]
async fn a_pause_is_refused_while_a_childs_turn_runs() {
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
                Play::Say("left it spinning"),
            ],
        )
        .plays(agent(1), [Play::Hang]);
    let family = Family {
        agents: agents.clone(),
        script,
        tools: &tools,
    };
    let mut saving = Saving {
        agents: &family.agents,
        at: vec![3],
        asked: Vec::new(),
        saved: None,
    };

    let (ending, _transcript) = family
        .run(
            requests,
            vec![user("go")],
            &mut saving,
            &mut DriveState::default(),
        )
        .await;

    assert_eq!(answered(&ending), Some("left it spinning"));
    assert_eq!(saving.asked, [(3, false)]);
    assert_eq!(saving.saved, None, "nothing was saved");
}
