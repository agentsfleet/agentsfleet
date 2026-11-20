//! A root that ends, or runs out of budget, ends its children; a root that
//! asks the person waits for them first.

use guffbolt::tools::ask::ASK_USER;

use super::*;

/// The root's loop ends while one child is idle and another still runs: the
/// running one ends interrupted with it, the idle one keeps its answer, and
/// neither is polled again.
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
                status: Kind::Done,
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
        "the running child ends with the run; the idle one keeps its answer"
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

/// A root that asks the person in the step that started a child ends waiting
/// only once that child has answered: the loop waits for the run's children
/// itself, so the child's turn is never dropped, and its answer outlives the
/// run's end.
#[tokio::test]
async fn a_question_waits_for_a_running_child_to_answer() {
    let (agents, requests) = Agents::new(CAPS);
    let names: Vec<&str> = TOOLS.into_iter().chain([ASK_USER]).collect();
    let config = ToolsConfig::new(&names).expect("eight names");
    let tools = Tools::builder()
        .with_agents(&agents)
        .build(&config)
        .expect("the run's children are given");
    let script = Script::default()
        .plays(
            ROOT,
            [Play::Call(vec![
                (
                    SPAWN_AGENT,
                    json!({"message": "check the build", "fork_turns": "none"}),
                ),
                (ASK_USER, json!({"question": "Ship it?"})),
            ])],
        )
        .plays(
            agent(1),
            [
                call(LIST_AGENTS, json!({})),
                Play::Say("the build is green"),
            ],
        );
    let family = Family {
        agents: agents.clone(),
        script,
        tools: &tools,
    };

    let (ending, _transcript) = family
        .run(
            requests,
            vec![user("go")],
            &mut Plain,
            &mut DriveState::default(),
        )
        .await;

    assert!(
        matches!(&ending, guffbolt::Ending::Waiting(waiting) if waiting.question == "Ship it?"),
        "{ending:?}"
    );
    assert_eq!(
        agents.wait(1, Some(std::time::Duration::ZERO)).await,
        Some(guffbolt::agents::Status::Done(
            "the build is green".to_owned()
        )),
        "the child ran to its answer before the run waited"
    );
    assert_eq!(family.script.sent_by(&agent(1)).len(), 2);
}
