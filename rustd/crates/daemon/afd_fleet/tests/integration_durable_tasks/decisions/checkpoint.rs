//! Publication advances a capture's head without replacing its verified base.

use std::time::Duration;

use af_core::{error_code, etag::sha256_hex};
use af_crypto::checkpoint_digest;
use af_wire::durable_task::{
    CheckpointCommitInput, CheckpointMode, CheckpointPage, CheckpointPageInput, CheckpointPlan,
    CheckpointPlanInput, DecisionDurableTaskVersion,
};
use afd_db::test_util::mint_id;
use afd_fleet::durable_task::{
    DurableTaskLifecycle as _, DurableTasks,
    storage::{CheckpointStorageConfig, R2CheckpointStore},
};

use super::{
    super::fixture::{Case, exercise, id},
    fixture::Decisions,
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn incremental_capture_retains_its_base_after_publication_and_refuses_incompatible_bases() {
    exercise(capture).await;
}

async fn capture(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let durable_task_id = id(&task.durable_task_id);
    super::steps::durable_tasks_enabled(case, false).await;
    let tasks = storage(&fixture);
    let base_input = input(
        &task,
        case,
        CheckpointMode::RetainBase,
        None,
        &"0".repeat(40),
    );
    let base = tasks
        .plan_checkpoint(case.run(), &valid(base_input))
        .await
        .expect("base allocation");
    let base = complete(&tasks, case, &task, &base).await;
    let head = "2".repeat(40);
    sqlx::query("UPDATE fleet.durable_tasks SET target = jsonb_set(target, ARRAY['head'], to_jsonb($2::text)), updated_at = now() WHERE id = $1")
        .bind(&durable_task_id).bind(&head)
        .execute(&mut *case.held.fixtures.database.acquire().await.expect("publication observer"))
        .await.expect("simulate the already confirmed publication head");
    let request = input(
        &task,
        case,
        CheckpointMode::Incremental,
        base.base_id.clone(),
        &head,
    );
    let plan = tasks
        .plan_checkpoint(case.run(), &valid(request.clone()))
        .await
        .expect("reuse older base");
    assert_eq!(
        plan.base_commit_oid.as_deref(),
        Some("0".repeat(40).as_str())
    );
    assert_eq!(plan.base_id, base.base_id);
    assert_eq!(plan.base_manifest_root, base.manifest_root);
    assert_eq!(plan.base_page_ids, base.page_ids);
    let committed = complete(&tasks, case, &task, &plan).await;
    let replay = tasks
        .plan_checkpoint(case.run(), &valid(request.clone()))
        .await
        .expect("immutable replay");
    assert!(replay.replayed);
    assert_eq!(replay.checkpoint_id, committed.checkpoint_id);
    assert_eq!(replay.manifest_root, committed.manifest_root);
    report_head(case, &task, &committed, &head).await;
    refusals(&tasks, case, request.clone()).await;
    delta(&tasks, case, &task, request, &committed).await;
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

pub(super) fn storage(fixture: &Decisions<'_>) -> DurableTasks {
    let case = fixture.case;
    let config = valid(CheckpointStorageConfig {
        endpoint: format!("https://{}.r2.cloudflarestorage.com", "0".repeat(32)),
        bucket: "checkpoint-test".to_owned(),
        credential_name: "checkpoint-test".to_owned(),
    });
    let client = afd_identity::direct::direct(Duration::from_secs(1))
        .build()
        .expect("bounded client");
    let mut dependencies = fixture.dependencies.clone();
    dependencies.storage = Some(R2CheckpointStore::new(
        config,
        case.scope.workspace_id.clone(),
        fixture.keys(),
        client,
    ));
    DurableTasks::new(case.held.runs.clone(), case.turns.clone(), dependencies)
}

pub(super) fn input(
    task: &DecisionDurableTaskVersion,
    case: &Case,
    mode: CheckpointMode,
    base_id: Option<String>,
    head: &str,
) -> CheckpointPlanInput {
    CheckpointPlanInput {
        fencing_token: case.run().fencing_token,
        operation_id: mint_id().into(),
        durable_task: task.clone(),
        mode,
        parent: None,
        base_id,
        commit_oid: Some(head.to_owned()),
        format_version: 1,
        toolbox_digest: "3".repeat(64),
        sandbox: None,
        page_count: 1,
        upload_budget_bytes: 0,
        logical_bytes: 0,
        entry_count: 0,
    }
}

pub(super) fn valid<T: garde::Validate<Context = ()>>(value: T) -> garde::Valid<T> {
    garde::Unvalidated::new(value)
        .validate()
        .expect("bounded fixture")
}

pub(super) async fn complete(
    tasks: &DurableTasks,
    case: &Case,
    task: &DecisionDurableTaskVersion,
    plan: &CheckpointPlan,
) -> CheckpointPlan {
    let contents = CheckpointPage {
        page_number: 0,
        entries: vec![],
        history_objects: vec![],
    };
    let digest = sha256_hex(&serde_json::to_vec(&contents).expect("manifest encoding"));
    let page = plan.page_ids.first().expect("one allocated page");
    let input = valid(CheckpointPageInput {
        fencing_token: case.run().fencing_token,
        durable_task: task.clone(),
        pin_id: plan.pin.id.clone(),
        digest: digest.clone(),
        contents,
    });
    tasks
        .write_checkpoint_page(case.run(), &id(&plan.checkpoint_id), &id(page), &input)
        .await
        .expect("freeze complete empty manifest");
    let input = valid(CheckpointCommitInput {
        fencing_token: case.run().fencing_token,
        durable_task: task.clone(),
        pin_id: plan.pin.id.clone(),
        manifest_root: checkpoint_digest::manifest_root([(page.as_str(), digest.as_str())]),
        history_digest: checkpoint_digest::history_root([]),
        covered_input_sequence: 0,
        effect_watermark: 0,
        uploaded: Vec::new(),
    });
    tasks
        .commit_checkpoint(case.run(), &id(&plan.checkpoint_id), &input)
        .await
        .expect("verified manifest commit")
}

async fn report_head(
    case: &Case,
    task: &DecisionDurableTaskVersion,
    plan: &CheckpointPlan,
    head: &str,
) {
    let durable_task_id = id(&task.durable_task_id);
    let checkpoint_id = id(&plan.checkpoint_id);
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("report observer");
    let state: (String, String) = sqlx::query_as("SELECT c.commit_oid, b.commit_oid FROM runner.checkpoints c JOIN runner.checkpoint_bases b ON b.id = c.checkpoint_base_id WHERE c.id = $1")
        .bind(&checkpoint_id).fetch_one(&mut *connection).await.expect("separate saved heads");
    assert_eq!(state, (head.to_owned(), "0".repeat(40)));
    for (candidate, expected) in [(head, true), ("0".repeat(40).as_str(), false)] {
        let accepted = afd_fleet::durable_task::CheckpointGuard {
            checkpoint_id: Some(plan.checkpoint_id.as_str()),
            durable_task_id: &durable_task_id,
            run_id: case.run().run_id,
            generation: 1,
            stop_generation: 0,
            covered_input_sequence: 0,
            effect_watermark: 0,
            head: Some(candidate),
        }
        .accepts(&mut connection)
        .await
        .expect("real report checkpoint guard");
        assert_eq!(accepted, expected);
    }
}

async fn refusals(tasks: &DurableTasks, case: &Case, request: CheckpointPlanInput) {
    for missing in [false, true] {
        let mut invalid = request.clone();
        invalid.operation_id = mint_id().into();
        if missing {
            invalid.base_id = Some(mint_id().into());
        } else {
            invalid.toolbox_digest = "4".repeat(64);
        }
        let failure = tasks
            .plan_checkpoint(case.run(), &valid(invalid))
            .await
            .expect_err("base compatibility required");
        assert_eq!(
            failure.code(),
            error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE
        );
    }
    // A delta is planned only over the task's current checkpoint.
    let mut invalid = request.clone();
    invalid.operation_id = mint_id().into();
    invalid.parent = Some(mint_id().into());
    let failure = tasks
        .plan_checkpoint(case.run(), &valid(invalid))
        .await
        .expect_err("a delta over another generation");
    assert_eq!(
        failure.code(),
        error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE
    );
    for commit in [None, Some("0".repeat(40))] {
        let mut invalid = request.clone();
        invalid.operation_id = mint_id().into();
        invalid.commit_oid = commit;
        let failure = tasks
            .plan_checkpoint(case.run(), &valid(invalid))
            .await
            .expect_err("current head required");
        assert_eq!(failure.code(), error_code::DURABLE_TASK_INVALID_TARGET);
    }
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM runner.checkpoints WHERE durable_task_id = $1")
            .bind(id(&request.durable_task.durable_task_id))
            .fetch_one(
                &mut *case
                    .held
                    .fixtures
                    .database
                    .acquire()
                    .await
                    .expect("residual observer"),
            )
            .await
            .expect("checkpoint count");
    assert_eq!(count, 2, "refused plans and replay allocate nothing");
}

/// A delta over the task's checkpoint names it as its parent and its chain,
/// and commits over it.
async fn delta(
    tasks: &DurableTasks,
    case: &Case,
    task: &DecisionDurableTaskVersion,
    mut request: CheckpointPlanInput,
    parent: &CheckpointPlan,
) {
    request.operation_id = mint_id().into();
    request.parent = Some(parent.checkpoint_id.clone());
    let plan = tasks
        .plan_checkpoint(case.run(), &valid(request))
        .await
        .expect("a delta over the current checkpoint");
    assert_eq!(plan.parent.as_ref(), Some(&parent.checkpoint_id));
    let links: Vec<_> = plan.chain.iter().map(|link| &link.checkpoint_id).collect();
    assert_eq!(links, [&parent.checkpoint_id]);
    assert_eq!(
        plan.chain.first().map(|link| &link.page_ids),
        Some(&parent.page_ids)
    );
    let committed = complete(tasks, case, task, &plan).await;
    assert_eq!(committed.parent, plan.parent);
    assert_eq!(committed.chain, plan.chain);
}
