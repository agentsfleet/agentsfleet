//! Behavior at the public root execution boundary.

#![expect(
    clippy::expect_used,
    reason = "test fixture: poisoned state must fail the test"
)]

use super::publication_fixture::*;
use super::*;
use afr_tools::sandbox::{ScriptedExecutor, ScriptedProcess};

#[tokio::test]
async fn validated_unchanged_candidate_publishes_only_native_target_and_reconciles_receipt()
-> crate::Result<()> {
    let native = publisher();
    let script = Script::new([vec![
        call(
            "publish",
            "propose_change",
            serde_json::json!({"validation": ["make", "test"], "timeout_seconds": 30}),
        ),
        ended(false),
    ]]);
    let executor = ScriptedExecutor::new([
        ScriptedProcess::exits(&candidate(), 0),
        ScriptedProcess::exits("checks passed", 0),
        ScriptedProcess::exits(&candidate(), 0),
    ]);
    let output = publish(
        &script,
        &native,
        &afr_executor::ExecutorSlot::ready(&executor),
    )
    .await?;
    let requests = native
        .published
        .lock()
        .expect("fixture mutex is unpoisoned");
    assert_eq!(requests.len(), 1);
    let request = requests.first().ok_or_else(crate::decision::unavailable)?;
    assert_candidate_request(request);
    let report = output.task.ok_or_else(crate::decision::unavailable)?;
    assert_eq!(report.effect_watermark, 1);
    assert_eq!(report.disposition, TaskDisposition::Waiting);
    assert_eq!(executor.spawned().len(), 3);
    assert_evidence(&native, "checks passed")?;
    assert_evidence(&native, "native_publication_receipt")?;
    assert!(
        script
            .sent()
            .first()
            .is_some_and(|sent| sent.tools.iter().any(|tool| tool == "propose_change"))
    );
    Ok(())
}

#[tokio::test]
async fn failed_incomplete_or_changed_validation_never_publishes() -> crate::Result<()> {
    for (processes, expected) in failed_validations() {
        let native = publisher();
        let script = Script::new([vec![
            call(
                "publish",
                "propose_change",
                serde_json::json!({"validation":["make","test"],"timeout_seconds":30}),
            ),
            ended(false),
        ]]);
        let executor = ScriptedExecutor::new(processes);
        let output = publish(
            &script,
            &native,
            &afr_executor::ExecutorSlot::ready(&executor),
        )
        .await?;
        assert!(
            native
                .published
                .lock()
                .expect("fixture mutex is unpoisoned")
                .is_empty()
        );
        assert_eq!(
            output
                .task
                .ok_or_else(crate::decision::unavailable)?
                .effect_watermark,
            0
        );
        assert_evidence(&native, expected)?;
    }
    Ok(())
}

#[tokio::test]
async fn invalid_publication_arguments_or_absent_workspace_refuse_before_spawn() -> crate::Result<()>
{
    for (arguments, expected) in invalid_calls() {
        let native = publisher();
        let script = Script::new([vec![
            call("publish", "propose_change", arguments),
            ended(false),
        ]]);
        let output = publish(
            &script,
            &native,
            &afr_executor::ExecutorSlot::unavailable(afr_executor::Unavailable::NoSandbox),
        )
        .await?;
        assert!(
            native
                .published
                .lock()
                .expect("fixture mutex is unpoisoned")
                .is_empty()
        );
        assert_eq!(
            output
                .task
                .ok_or_else(crate::decision::unavailable)?
                .effect_watermark,
            0
        );
        let requests = native
            .goals
            .requests
            .lock()
            .expect("fixture mutex is unpoisoned");
        assert!(
            requests
                .first()
                .ok_or_else(crate::decision::unavailable)?
                .evidence
                .to_string()
                .contains(expected)
        );
    }
    Ok(())
}

#[tokio::test]
async fn api_only_task_cannot_publish_even_when_model_invents_the_tool_call() -> crate::Result<()> {
    let mut native = publisher();
    if let Some(task) = native.goals.inputs.current_task.as_mut() {
        task.target = TaskTarget::ApiOnly;
    }
    let script = Script::new([vec![
        call(
            "publish",
            "propose_change",
            serde_json::json!({"validation":["make"],"timeout_seconds":30}),
        ),
        ended(false),
    ]]);
    let executor = ScriptedExecutor::new([]);
    let output = publish(
        &script,
        &native,
        &afr_executor::ExecutorSlot::ready(&executor),
    )
    .await?;
    assert_eq!(
        output
            .task
            .ok_or_else(crate::decision::unavailable)?
            .effect_watermark,
        0
    );
    assert_eq!(executor.spawned().len(), 0);
    assert!(
        native
            .published
            .lock()
            .expect("fixture mutex is unpoisoned")
            .is_empty()
    );
    assert!(
        script
            .sent()
            .first()
            .is_some_and(|request| request.tools.iter().all(|name| name != "propose_change"))
    );
    assert_evidence(&native, "Publication is unavailable for this task")?;
    Ok(())
}

fn failed_validations() -> Vec<(Vec<ScriptedProcess>, &'static str)> {
    let changed = serde_json::json!({"tested_tree": "e".repeat(40), "changes": [{"path":"src/lib.rs", "contents":"b3RoZXIK"}]}).to_string();
    vec![
        (
            vec![ScriptedProcess::exits("invalid json", 0)],
            "complete supported candidate",
        ),
        (
            vec![ScriptedProcess::ends_abandoned(
                &candidate(),
                afr_executor::Ending::Exited(0),
            )],
            "capture was incomplete",
        ),
        (
            vec![
                ScriptedProcess::exits(&candidate(), 0),
                ScriptedProcess::exits("test failed", 1),
            ],
            "test failed",
        ),
        (
            vec![
                ScriptedProcess::exits(&candidate(), 0),
                ScriptedProcess::exits("checks passed", 0),
                ScriptedProcess::exits(&changed, 0),
            ],
            "Validation changed the candidate tree",
        ),
        (
            vec![ScriptedProcess::exits(
                &serde_json::json!({"tested_tree":"b".repeat(40),"changes":[]}).to_string(),
                0,
            )],
            "no local changes",
        ),
        (
            vec![ScriptedProcess::vanishes("partial")],
            "without a complete result",
        ),
    ]
}

fn assert_candidate_request(request: &TaskEffectInput) {
    assert_eq!(request.expected_head, "a".repeat(40));
    assert_eq!(request.tested_tree, "b".repeat(40));
    assert_eq!(request.source_item_id, ITEM);
    assert_eq!(request.task.task_id, SOURCE);
    assert_eq!(
        request.changes,
        vec![TaskFileChange {
            path: "src/lib.rs".to_owned(),
            contents: Some("cmVwYWlyCg==".to_owned())
        }]
    );
}

fn invalid_calls() -> Vec<(serde_json::Value, &'static str)> {
    vec![
        (
            serde_json::json!({"validation":[],"timeout_seconds":30}),
            "exceed their bounds",
        ),
        (
            serde_json::json!({"validation":["make"],"timeout_seconds":0}),
            "exceed their bounds",
        ),
        (
            serde_json::json!({"validation":["make"],"timeout_seconds":30,"repository":"other/project"}),
            "bounded validation command",
        ),
        (
            serde_json::json!({"validation":["make"],"timeout_seconds":30}),
            "requires the task workspace",
        ),
    ]
}

fn assert_evidence(native: &Publisher, expected: &str) -> crate::Result<()> {
    let requests = native
        .goals
        .requests
        .lock()
        .expect("fixture mutex is unpoisoned");
    let evidence = requests
        .first()
        .ok_or_else(crate::decision::unavailable)?
        .evidence
        .to_string();
    assert!(evidence.contains(expected), "{evidence}");
    Ok(())
}
