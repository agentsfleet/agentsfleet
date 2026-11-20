//! A root that ends, or runs out of budget, ends its children.

use super::*;

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
