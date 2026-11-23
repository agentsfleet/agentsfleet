//! A loop over built tools calls them, answers a refusal and ends.

use super::*;

#[tokio::test]
async fn a_run_over_built_tools_calls_them_and_ends() {
    let memory = LocalMemory::default();
    let config = ToolsConfig::new(["memory_*", "echo"]).expect("two entries");
    let tools: Tools<ContextTool<'_>> = Tools::builder()
        .with_memory(&memory)
        .with_tool(echo("echo"))
        .build(&config)
        .expect("built");
    let (agents, requests) = Agents::new(CAPS);
    let script = Script::default().plays(
        ROOT,
        [
            call(
                "memory_store",
                json!({"key": "deploy:window", "content": "Fridays after six"}),
            ),
            Play::Call(vec![
                ("memory_recall", json!({"query": "DEPLOY"})),
                ("echo", json!({"n": 1})),
            ]),
            Play::Say("deploys go out on Fridays after six"),
        ],
    );
    let family = Family {
        agents,
        script,
        tools: &tools,
    };

    let (ending, transcript) = family
        .run(
            requests,
            vec![user("when do we deploy?")],
            &mut Plain,
            &mut DriveState::default(),
        )
        .await;

    assert_eq!(
        answered(&ending),
        Some("deploys go out on Fridays after six")
    );
    assert_eq!(
        results(&transcript),
        [
            "stored deploy:window",
            "deploy:window (core): Fridays after six",
            r#"echo {"n":1} at step 2 call 1"#,
        ]
    );
    assert_eq!(
        memory.entries(),
        [Memory {
            key: "deploy:window".to_owned(),
            content: "Fridays after six".to_owned(),
            category: "core".to_owned(),
            reach: guffbolt::tools::memory::Reach::Private,
        }]
    );
    assert_eq!(
        family
            .script
            .sent_by(ROOT)
            .first()
            .map(|sent| sent.tools.clone()),
        Some(
            [
                "memory_store",
                "memory_recall",
                "memory_list",
                "memory_forget",
                "echo"
            ]
            .map(str::to_owned)
            .to_vec()
        ),
        "offered in the config's order"
    );
}

/// A call whose arguments break the tool's schema, and one to a name the run
/// holds no tool for, answer the model; the loop goes on.
#[tokio::test]
async fn a_refused_call_through_the_builder_answers_the_model_and_the_loop_continues() {
    let memory = LocalMemory::default();
    let config = ToolsConfig::new(["memory_store"]).expect("a name");
    let tools = Tools::builder()
        .with_memory(&memory)
        .build(&config)
        .expect("built");
    let (agents, requests) = Agents::new(CAPS);
    let script = Script::default().plays(
        ROOT,
        [
            Play::Call(vec![
                ("memory_store", json!({"key": "k"})),
                (
                    "memory_store",
                    json!({"key": "k", "content": "c", "colour": "red"}),
                ),
                ("memory_store", json!("not an object")),
                ("memory_recall", json!({"query": "k"})),
            ]),
            call(
                "memory_store",
                json!({"key": "k", "content": "\u{1F680} launch"}),
            ),
            Play::Say("stored it the second time"),
        ],
    );
    let family = Family {
        agents,
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

    assert_eq!(answered(&ending), Some("stored it the second time"));
    let read = results(&transcript);
    assert_eq!(read.len(), 5);
    assert_eq!(
        read.first().map(String::as_str),
        Some("[invalid_arguments] missing field `content`")
    );
    assert!(
        read.get(1).is_some_and(
            |refusal| refusal.starts_with("[invalid_arguments] unknown field `colour`")
        ),
        "{read:?}"
    );
    assert!(
        read.get(2)
            .is_some_and(|refusal| refusal.starts_with("[invalid_arguments] invalid type: string")),
        "{read:?}"
    );
    assert_eq!(
        read.get(3).map(String::as_str),
        Some("[tool_not_offered] memory_recall is not one of this run's tools"),
        "a tool the config did not name is not held"
    );
    assert_eq!(read.get(4).map(String::as_str), Some("stored k"));
    assert_eq!(
        memory
            .entries()
            .iter()
            .map(|entry| entry.content.as_str())
            .collect::<Vec<_>>(),
        ["\u{1F680} launch"],
        "only the call that parsed stored anything"
    );
}
