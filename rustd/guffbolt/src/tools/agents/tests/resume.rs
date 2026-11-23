//! A resumed loop restores its children from its drive state.

use super::*;

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
