//! `ask_user`, `update_plan`, `current_time` and `sleep`.

use super::*;

/// `ask_user` ends the root's run waiting on its question, its options
/// numbered; a child that asks is told it cannot, and its turn fails.
#[tokio::test]
async fn ask_user_waits_at_the_root_and_fails_a_childs_turn() {
    let (agents, requests) = Agents::new(Caps {
        depth: 2,
        open: 1,
        started: 1,
    });
    let mut names: Vec<&str> = TOOLS.to_vec();
    names.push("ask_user");
    let tools = built(Tools::builder().with_agents(&agents), &names);
    let script = Script::default()
        .plays(
            ROOT,
            [
                call(
                    SPAWN_AGENT,
                    json!({"message": "pick", "fork_turns": "none"}),
                ),
                call(WAIT_AGENT, json!({"target": 1})),
                call(
                    "ask_user",
                    json!({"question": "Which region?", "options": ["eu", "us"]}),
                ),
                Play::Say("eu it is"),
            ],
        )
        .plays(agent(1), [call("ask_user", json!({"question": "Which?"}))]);
    let family = Family {
        agents: agents.clone(),
        script,
        tools: &tools,
    };
    let mut state = DriveState::default();

    let (ending, transcript) = family
        .run(requests, vec![user("deploy")], &mut Plain, &mut state)
        .await;

    let waiting = match ending {
        Ending::Waiting(waiting) => Some(waiting),
        Ending::Answered(_) | Ending::Failed(_) | Ending::Stopped => None,
    }
    .expect("the root waits on its question");
    assert_eq!(waiting.question, "Which region?\n\n1. eu\n2. us");
    assert_eq!(
        results(&transcript).get(1).map(String::as_str),
        Some(
            r#"{"status":"failed","detail":"asked a question only the person can answer: Which?"}"#
        )
    );
    assert_eq!(state.waiting(), Some(&waiting));
}

#[tokio::test]
async fn update_plan_reads_back_the_plan_and_refuses_an_unknown_status() {
    let tools = built(Tools::builder(), &["update_plan"]);

    let recorded = run(
        &tools,
        "update_plan",
        json!({"explanation": "found the cause", "plan": [
            {"step": "read logs", "status": "completed"},
            {"step": "fix the parser", "status": "in_progress"},
            {"step": "ship", "status": "pending"}
        ]}),
        None,
    )
    .await;
    let bare = run(&tools, "update_plan", json!({"plan": []}), None).await;
    let refused = run(
        &tools,
        "update_plan",
        json!({"plan": [{"step": "x", "status": "blocked"}]}),
        None,
    )
    .await;
    let missing = run(&tools, "update_plan", json!({}), None).await;

    assert_eq!(
        recorded.text,
        "found the cause\n[completed] read logs\n[in_progress] fix the parser\n[pending] ship"
    );
    assert_eq!(bare.text, "");
    assert_eq!(refused.error_code, Some(code::INVALID_ARGUMENTS));
    assert!(
        refused
            .text
            .starts_with("[invalid_arguments] unknown variant `blocked`"),
        "{}",
        refused.text
    );
    assert_eq!(missing.text, "[invalid_arguments] missing field `plan`");
}

#[tokio::test]
async fn current_time_reads_an_rfc_3339_instant_in_utc() {
    let tools = built(Tools::builder(), &["current_time"]);

    let read = run(&tools, "current_time", json!({}), None).await;
    let refused = run(&tools, "current_time", json!({"zone": "IST"}), None).await;

    let instant: jiff::Timestamp = read.text.parse().expect("RFC 3339");
    assert!(read.text.ends_with('Z'), "{}", read.text);
    assert!(instant > "2026-01-01T00:00:00Z".parse().expect("an instant"));
    assert_eq!(refused.error_code, Some(code::INVALID_ARGUMENTS));
}

/// `sleep` waits as asked, never past ten minutes, and wakes at once when
/// input arrived first. The clock is tokio's paused one, so no test waits.
#[tokio::test(start_paused = true)]
async fn sleep_waits_as_asked_capped_at_ten_minutes_and_wakes_for_input() {
    let tools = built(Tools::builder(), &["sleep"]);
    let wake = Wake::default();

    let slept = run(&tools, "sleep", json!({"duration_ms": 1_500}), None).await;
    let capped = run(
        &tools,
        "sleep",
        json!({"duration_ms": 86_400_000}),
        Some(&wake),
    )
    .await;
    wake.input_arrived();
    let woken_early = run(&tools, "sleep", json!({"duration_ms": 60_000}), Some(&wake)).await;
    let refused = run(&tools, "sleep", json!({"duration_ms": -1}), None).await;

    assert_eq!(slept.text, "Slept 1500 ms");
    assert_eq!(capped.text, "Slept 600000 ms");
    assert_eq!(woken_early.text, "Woke after 0 ms: a new message arrived");
    assert_eq!(refused.error_code, Some(code::INVALID_ARGUMENTS));
}
