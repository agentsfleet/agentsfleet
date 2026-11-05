//! Publication advances a capture's head without replacing its verified base.

use std::{sync::Arc, time::Duration};

use afd_core::{error_code, etag::sha256_hex};
use afd_crypto::{checkpoint_digest, secret::Kek};
use afd_db::test_util::mint_id;
use afd_fleet::task::{
    TaskLifecycle as _, Tasks,
    storage::{CheckpointStorageConfig, R2CheckpointStore},
};
use afd_wire::task::{
    CheckpointCommitInput, CheckpointMode, CheckpointPage, CheckpointPageInput, CheckpointPlan,
    CheckpointPlanInput, DecisionTaskVersion,
};

use super::{
    super::fixture::{Case, exercise, id},
    fixture::Decisions,
};
use crate::report_seed::FIXTURE_KEK_HEX;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn incremental_capture_retains_its_base_after_publication_and_refuses_incompatible_bases() {
    exercise(capture).await;
}

async fn capture(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let tasks = storage(&fixture);
    let base_input = input(
        &task,
        case,
        CheckpointMode::RetainBase,
        None,
        &"0".repeat(40),
    );
    let base = tasks
        .plan_checkpoint(case.lease(), &valid(base_input))
        .await
        .expect("base allocation");
    let base = complete(&tasks, case, &task, &base).await;
    let head = "2".repeat(40);
    sqlx::query("UPDATE fleet.tasks SET target = jsonb_set(target, '{head}', to_jsonb($2::text)), updated_at = now() WHERE id = $1::uuid")
        .bind(&task.task_id).bind(&head)
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
        .plan_checkpoint(case.lease(), &valid(request.clone()))
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
        .plan_checkpoint(case.lease(), &valid(request.clone()))
        .await
        .expect("immutable replay");
    assert!(replay.replayed);
    assert_eq!(replay.checkpoint_id, committed.checkpoint_id);
    assert_eq!(replay.manifest_root, committed.manifest_root);
    report_head(case, &task, &committed, &head).await;
    refusals(&tasks, case, request).await;
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

fn storage(fixture: &Decisions<'_>) -> Tasks {
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
        case.held.fixtures.plane().vault,
        Arc::new(Kek::from_hex(FIXTURE_KEK_HEX).expect("fixture key")),
        client,
    ));
    Tasks::new(
        case.held.leases.clone(),
        case.admissions.clone(),
        dependencies,
    )
}

fn input(
    task: &DecisionTaskVersion,
    case: &Case,
    mode: CheckpointMode,
    base_id: Option<String>,
    head: &str,
) -> CheckpointPlanInput {
    CheckpointPlanInput {
        fencing_token: case.lease().fencing_token,
        operation_id: mint_id(),
        task: task.clone(),
        mode,
        base_id,
        commit_oid: Some(head.to_owned()),
        format_version: 1,
        toolbox_digest: "3".repeat(64),
        page_count: 1,
        upload_budget_bytes: 0,
        logical_bytes: 0,
        entry_count: 0,
    }
}

fn valid<T: garde::Validate<Context = ()>>(value: T) -> garde::Valid<T> {
    garde::Unvalidated::new(value)
        .validate()
        .expect("bounded fixture")
}

async fn complete(
    tasks: &Tasks,
    case: &Case,
    task: &DecisionTaskVersion,
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
        fencing_token: case.lease().fencing_token,
        task: task.clone(),
        pin_id: plan.pin.id.clone(),
        digest: digest.clone(),
        contents,
    });
    tasks
        .write_checkpoint_page(case.lease(), &id(&plan.checkpoint_id), &id(page), &input)
        .await
        .expect("freeze complete empty manifest");
    let input = valid(CheckpointCommitInput {
        fencing_token: case.lease().fencing_token,
        task: task.clone(),
        pin_id: plan.pin.id.clone(),
        manifest_root: checkpoint_digest::manifest_root([(page.as_str(), digest.as_str())]),
        history_digest: checkpoint_digest::history_root([]),
        covered_input_sequence: 0,
        effect_watermark: 0,
    });
    tasks
        .commit_checkpoint(case.lease(), &id(&plan.checkpoint_id), &input)
        .await
        .expect("verified manifest commit")
}

async fn report_head(case: &Case, task: &DecisionTaskVersion, plan: &CheckpointPlan, head: &str) {
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("report observer");
    let state: (String, String) = sqlx::query_as("SELECT c.commit_oid, b.commit_oid FROM fleet.task_checkpoints c JOIN fleet.task_bases b ON b.id = c.base_id WHERE c.id = $1::uuid")
        .bind(&plan.checkpoint_id).fetch_one(&mut *connection).await.expect("separate saved heads");
    assert_eq!(state, (head.to_owned(), "0".repeat(40)));
    let revision: String = sqlx::query_scalar(
        "SELECT accepted_revision_id::text FROM fleet.tasks WHERE id = $1::uuid",
    )
    .bind(&task.task_id)
    .fetch_one(&mut *connection)
    .await
    .expect("accepted revision");
    for (candidate, expected) in [(head, true), ("0".repeat(40).as_str(), false)] {
        let accepted = afd_fleet::task::CheckpointGuard {
            checkpoint_id: Some(&plan.checkpoint_id),
            task_id: &task.task_id,
            lease_id: case.lease().lease_id.as_str(),
            generation: 1,
            stop_generation: 0,
            revision_id: &revision,
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

async fn refusals(tasks: &Tasks, case: &Case, request: CheckpointPlanInput) {
    for missing in [false, true] {
        let mut invalid = request.clone();
        invalid.operation_id = mint_id();
        if missing {
            invalid.base_id = Some(mint_id());
        } else {
            invalid.toolbox_digest = "4".repeat(64);
        }
        let failure = tasks
            .plan_checkpoint(case.lease(), &valid(invalid))
            .await
            .expect_err("base compatibility required");
        assert_eq!(failure.code(), error_code::TASK_CHECKPOINT_INCOMPATIBLE);
    }
    for commit in [None, Some("0".repeat(40))] {
        let mut invalid = request.clone();
        invalid.operation_id = mint_id();
        invalid.commit_oid = commit;
        let failure = tasks
            .plan_checkpoint(case.lease(), &valid(invalid))
            .await
            .expect_err("current head required");
        assert_eq!(failure.code(), error_code::TASK_INVALID_TARGET);
    }
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fleet.task_checkpoints WHERE task_id = $1::uuid")
            .bind(&request.task.task_id)
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
