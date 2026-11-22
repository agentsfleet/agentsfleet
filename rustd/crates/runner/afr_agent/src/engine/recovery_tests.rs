//! Recovery preserves work while refusing historical authority and new secrets.

use super::RootRecovery;
use super::recovery::TaskRecovery;
use af_wire::task::{TaskContext, TaskLimits, TaskState, TaskTarget, TaskUsage};
use guffbolt::{Call, Message, Replay};

pub(super) fn task() -> TaskContext<'static> {
    TaskContext {
        task_id: "019f0000-0000-7000-8000-000000000001".into(),
        originating_item_id: "019f0000-0000-7000-8000-000000000002".into(),
        target: TaskTarget::ApiOnly,
        generation: 1,
        stop_generation: 0,
        accepted_revision_id: "019f0000-0000-7000-8000-000000000003".into(),
        objective: "Investigate the reported failure".into(),
        goal_digest: "a".repeat(64).into(),
        questions: vec![],
        state: TaskState::Running,
        covered_input_sequence: 2,
        input_sequence: 2,
        effect_cursor: 0,
        confirmed_effect_watermark: 0,
        checkpoint_id: None,
        checkpoint_generation: None,
        expires_at: jiff::Timestamp::UNIX_EPOCH,
        limits: TaskLimits::default(),
        usage: TaskUsage::default(),
    }
}

#[test]
fn adopted_goal_restores_prior_work_as_scrubbed_data_but_cannot_change_task_identity()
-> crate::Result<()> {
    let original = task();
    let messages = prior_work();
    let saved = TaskRecovery::capture(
        original.clone(),
        "old-model",
        "old-provider",
        RootRecovery {
            messages,
            ..RootRecovery::default()
        },
    )?;
    let mut adopted = original;
    adopted.goal_digest = "b".repeat(64).into();
    adopted.generation += 1;
    let restored = TaskRecovery::restore(
        &saved.history,
        &adopted,
        "new-model",
        "new-provider",
        &crate::fixture::scrub(),
    )?;
    let encoded = serde_json::to_string(&restored)?;
    assert!(!encoded.contains(crate::fixture::API_KEY));
    assert!(!encoded.contains(crate::fixture::GITHUB_TOKEN));
    assert!(encoded.contains("Observed the failure"));
    assert_eq!(restored.messages.len(), 3);
    assert_eq!(adopted.generation, 2);
    adopted.task_id = "019f0000-0000-7000-8000-000000000004".into();
    let failure = TaskRecovery::restore(
        &saved.history,
        &adopted,
        "old-model",
        "old-provider",
        &crate::fixture::scrub(),
    )
    .err()
    .ok_or_else(crate::decision::unavailable)?;
    assert_eq!(
        failure.code(),
        af_core::error_code::RUN_CHECKPOINT_INCOMPATIBLE
    );
    Ok(())
}

#[test]
fn incomplete_or_unknown_history_never_becomes_an_empty_successful_resume() -> crate::Result<()> {
    let task = task();
    let saved = TaskRecovery::capture(
        task.clone(),
        "model",
        "provider",
        RootRecovery {
            messages: vec![Message::User("retained work".to_owned())],
            ..RootRecovery::default()
        },
    )?;
    let mut history: serde_json::Value = serde_json::from_slice(&saved.history)?;
    *history
        .get_mut("format")
        .ok_or_else(crate::decision::unavailable)? = serde_json::json!(1);
    let failure = TaskRecovery::restore(
        &serde_json::to_vec(&history)?,
        &task,
        "model",
        "provider",
        &crate::fixture::scrub(),
    )
    .err()
    .ok_or_else(crate::decision::unavailable)?;
    assert_eq!(
        failure.code(),
        af_core::error_code::RUN_CHECKPOINT_INCOMPATIBLE
    );
    let failure = TaskRecovery::restore(b"{", &task, "model", "provider", &crate::fixture::scrub())
        .err()
        .ok_or_else(crate::decision::unavailable)?;
    assert_eq!(
        failure.code(),
        af_core::error_code::RUN_CHECKPOINT_INCOMPATIBLE
    );
    assert!(!format!("{saved:?}").contains("retained work"));
    Ok(())
}

#[test]
fn rich_history_retains_driver_answers_and_pinned_input_index() -> crate::Result<()> {
    let text = format!("retained answer {}", crate::fixture::API_KEY);
    let driver =
        serde_json::from_value(serde_json::json!({"answers": [text], "completed_steps": 1}))?;
    let root = RootRecovery {
        messages: vec![
            Message::User("request".to_owned()),
            Message::Assistant {
                text,
                calls: vec![],
                replay: Replay::default(),
            },
        ],
        driver,
        pinned: 1,
        ..RootRecovery::default()
    };
    let task = task();
    let saved = TaskRecovery::capture(task.clone(), "model", "provider", root)?;
    let recovered = TaskRecovery::restore(
        &saved.history,
        &task,
        "model",
        "provider",
        &crate::fixture::scrub(),
    )?;
    assert_eq!(recovered.driver.completed_steps(), 1);
    assert_eq!(recovered.pinned, 1);
    assert_eq!(recovered.driver.answers().len(), 1);
    assert!(
        !recovered
            .driver
            .answers()
            .iter()
            .any(|answer| answer.contains(crate::fixture::API_KEY))
    );
    Ok(())
}

fn prior_work() -> Vec<Message> {
    vec![
        Message::User(format!("old input {}", crate::fixture::API_KEY)),
        Message::Assistant {
            text: "Observed the failure".to_owned(),
            calls: vec![Call {
                id: "c1".to_owned(),
                name: "shell".to_owned(),
                arguments: serde_json::json!({"command": crate::fixture::GITHUB_TOKEN}),
            }],
            replay: Replay::default(),
        },
        Message::ToolResult {
            call_id: "c1".to_owned(),
            output: format!("result {}", crate::fixture::GITHUB_TOKEN),
            image: None,
        },
    ]
}

/// The bytes past which private task history is refused: 64 MiB.
const HISTORY_LIMIT: usize = 64 * 1024 * 1024;

#[test]
fn history_past_its_byte_limit_is_neither_captured_nor_restored() -> crate::Result<()> {
    let task = task();
    let captured = TaskRecovery::capture(
        task.clone(),
        "model",
        "provider",
        RootRecovery {
            messages: vec![Message::User("x".repeat(HISTORY_LIMIT))],
            ..RootRecovery::default()
        },
    );
    let restored = TaskRecovery::restore(
        &vec![b' '; HISTORY_LIMIT + 1],
        &task,
        "model",
        "provider",
        &crate::fixture::scrub(),
    );

    for refused in [captured.err(), restored.err()] {
        let failure = refused.ok_or_else(crate::decision::unavailable)?;
        assert_eq!(
            failure.code(),
            af_core::error_code::RUN_CHECKPOINT_INCOMPATIBLE
        );
        assert!(
            failure
                .to_string()
                .contains("private task history exceeds its byte limit"),
            "{failure}"
        );
    }
    Ok(())
}

#[test]
fn task_history_carrying_ordinary_folds_is_not_restored() -> crate::Result<()> {
    let task = task();
    let saved = TaskRecovery::capture(
        task.clone(),
        "model",
        "provider",
        RootRecovery {
            messages: vec![Message::User("retained work".to_owned())],
            folds: vec![af_wire::run::FoldRef {
                turn_id: af_wire::run::CheckpointOperationId(
                    "019f0000-0000-7000-8000-000000000007".to_owned(),
                ),
                sequence: 1,
            }],
            ..RootRecovery::default()
        },
    )?;

    let failure = TaskRecovery::restore(
        &saved.history,
        &task,
        "model",
        "provider",
        &crate::fixture::scrub(),
    )
    .err()
    .ok_or_else(crate::decision::unavailable)?;

    assert_eq!(
        failure.code(),
        af_core::error_code::RUN_CHECKPOINT_INCOMPATIBLE
    );
    assert!(
        failure
            .to_string()
            .contains("native task history cannot contain ordinary folds"),
        "{failure}"
    );
    Ok(())
}

/// A child the root ran, ended `status`, its transcript and answers carrying
/// the run's key.
fn child(id: u64, status: guffbolt::agents::Status) -> crate::Result<guffbolt::agents::Counted> {
    Ok(guffbolt::agents::Counted {
        id,
        parent: 0,
        depth: 1,
        calls: 1,
        tools: vec![],
        steps: 0,
        status,
        transcript: vec![Message::User(format!(
            "child task with {}",
            crate::fixture::API_KEY
        ))],
        state: serde_json::from_value(serde_json::json!({
            "answers": [format!("child answer {}", crate::fixture::API_KEY)],
            "completed_steps": 1
        }))?,
    })
}

/// A root whose driver carries `children` under the census `next` and
/// `started`.
fn with_children(
    next: u64,
    started: u64,
    children: Vec<guffbolt::agents::Counted>,
) -> RootRecovery {
    let mut root = RootRecovery {
        messages: vec![Message::User("retained work".to_owned())],
        ..RootRecovery::default()
    };
    *root.driver.children_mut() = guffbolt::agents::Census {
        next,
        started,
        children,
    };
    root
}

#[test]
fn ended_children_restore_with_every_secret_they_carried_masked() -> crate::Result<()> {
    use guffbolt::agents::Status;

    let task = task();
    let secret = |text: &str| format!("{text} {}", crate::fixture::API_KEY);
    let root = with_children(
        2,
        2,
        vec![
            child(1, Status::Done(secret("child done")))?,
            child(2, Status::Failed(secret("child failed")))?,
        ],
    );
    let saved = TaskRecovery::capture(task.clone(), "model", "provider", root)?;

    let restored = TaskRecovery::restore(
        &saved.history,
        &task,
        "model",
        "provider",
        &crate::fixture::scrub(),
    )?;

    let children = &restored.driver.children().children;
    assert_eq!(children.len(), 2);
    let encoded = serde_json::to_string(children)?;
    assert!(!encoded.contains(crate::fixture::API_KEY), "{encoded}");
    for kept in [
        "child done",
        "child failed",
        "child task with",
        "child answer",
    ] {
        assert!(encoded.contains(kept), "{kept} was lost: {encoded}");
    }
    Ok(())
}

#[test]
fn a_root_whose_saved_state_contradicts_itself_is_not_captured() -> crate::Result<()> {
    use guffbolt::agents::Status;

    let fold = |number: u32, sequence: i64| af_wire::run::FoldRef {
        turn_id: af_wire::run::CheckpointOperationId(format!(
            "019f0000-0000-7000-8000-{number:012x}"
        )),
        sequence,
    };
    let unordered = RootRecovery {
        messages: vec![Message::User("retained work".to_owned())],
        folds: vec![fold(7, 2), fold(8, 1)],
        ..RootRecovery::default()
    };
    let overpinned = RootRecovery {
        messages: vec![Message::User("retained work".to_owned())],
        pinned: 2,
        ..RootRecovery::default()
    };
    let miscounted = with_children(2, 1, vec![child(1, Status::Done("done".to_owned()))?]);
    let running = with_children(1, 1, vec![child(1, Status::Running)?]);
    let cases = [
        (
            unordered,
            "covered fold identities or order are inconsistent",
        ),
        (
            overpinned,
            "root driver state or pinned input index is inconsistent",
        ),
        (
            miscounted,
            "terminal child identifiers or counters are inconsistent",
        ),
        (
            running,
            "a saved child is running, out of order or past its bounds",
        ),
    ];

    for (root, reason) in cases {
        let failure = TaskRecovery::capture(task(), "model", "provider", root)
            .err()
            .ok_or_else(crate::decision::unavailable)?;

        assert_eq!(
            failure.code(),
            af_core::error_code::RUN_CHECKPOINT_INCOMPATIBLE
        );
        assert!(failure.to_string().contains(reason), "{failure}");
    }
    Ok(())
}
