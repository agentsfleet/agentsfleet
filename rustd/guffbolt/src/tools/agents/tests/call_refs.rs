//! A child's calls keep their places unique across a turn the run's end
//! dropped and a run resumed from the saved state.

use std::collections::HashSet;

use super::*;
use crate::agents::INTERRUPT_AGENT;

#[tokio::test(start_paused = true)]
async fn no_call_ref_repeats_across_a_dropped_turn_and_a_restore() {
    let (agents, requests) = Agents::new(CAPS);
    let built = tools(&agents);
    let puppet = Puppet::default()
        .plays(
            ROOT,
            [
                call(
                    SPAWN_AGENT,
                    json!({"message": "look", "fork_turns": "none"}),
                ),
                call(WAIT_AGENT, json!({"target": 1, "timeout_ms": 50})),
                Play::Say("leaving"),
            ],
        )
        .plays(agent(1), [call(LIST_AGENTS, json!({})), Play::Hang]);
    let family = Family {
        agents: agents.clone(),
        puppet,
        tools: &built,
    };
    let mut state = DriveState::default();
    let mut root = family.root(vec![user("go")]);
    let mut children = Children::new(requests);
    let first = children
        .beside(&family, drive(&mut root, &mut Plain, &mut state))
        .await;
    // The child's turn hangs on its second step, so the run's end drops it.
    let pause = children.end_all(&family.agents);
    let saved = pause
        .as_ref()
        .is_some_and(|pause| family.agents.save(pause, &mut state));
    drop(pause);
    let encoded = serde_json::to_string(&state).expect("a state encodes");
    let mut decoded: DriveState = serde_json::from_str(&encoded).expect("and decodes");

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
        .plays(
            agent(1),
            [
                call(INTERRUPT_AGENT, json!({"target": 9})),
                Play::Say("looked"),
            ],
        );
    let resumed = Family {
        agents: restored.clone(),
        puppet,
        tools: &rebuilt,
    };
    let (ending, _transcript) =
        run_root(&resumed, requests, vec![user("go on")], &mut decoded).await;

    assert_eq!(answered(&first), Some("leaving"));
    assert!(saved, "the quiet set was saved");
    assert!(
        answered(&ending).is_some_and(|text| text.ends_with("resumed")),
        "{ending:?}"
    );
    let lent: Vec<(u64, u64)> = family
        .puppet
        .lent_by(&agent(1))
        .into_iter()
        .chain(resumed.puppet.lent_by(&agent(1)))
        .map(|call_ref| (call_ref.step, call_ref.index))
        .collect();
    assert_eq!(
        lent,
        [(1, 0), (2, 0)],
        "the dropped turn's step is not lent again"
    );
    assert_eq!(
        lent.iter().collect::<HashSet<_>>().len(),
        lent.len(),
        "no place repeats"
    );
}
