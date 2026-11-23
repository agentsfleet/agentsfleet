//! A plan is refused when storage is absent, its mode and base disagree, or its
//! target names no repository; a ready base for the same commit is reused.

use af_core::error_code;
use af_crypto::checkpoint_digest;
use af_wire::durable_task::{
    CheckpointCommitInput, CheckpointMode, CheckpointPlan, DecisionDurableTaskVersion,
};
use afd_db::test_util::mint_id;
use afd_fleet::durable_task::{DurableTaskLifecycle as _, DurableTasks};

use super::{
    super::{
        super::fixture::{Case, exercise, id},
        checkpoint::{complete, input, storage, valid},
        fixture::Decisions,
    },
    Pages, Stored, api_only, digest_of, directory, incremental, page, planned, refused, write,
};

/// The head the fixture task's pull request is at.
fn head() -> String {
    "0".repeat(40)
}

// Value: protects=a capture is planned only where storage is configured, its mode agrees
// with its base, an unfinished base is never shared, and an overlay never publishes over
// a base altered at rest; fails_when=any of the storage, mode-and-base or unready-base
// refusals is dropped from `plan_checkpoint`, a ready base stops being reused, or the
// commit stops re-proving the base root; why_new=these plan arms had no case; seam=the production task
// verbs over live Postgres, the fixture task's real pull-request target.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_plan_whose_storage_mode_or_base_cannot_hold_is_refused_and_a_ready_base_is_reused() {
    exercise(repository).await;
}

async fn repository(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let base_request = || input(&task, case, CheckpointMode::RetainBase, None, &head());

    refused(
        fixture
            .tasks
            .plan_checkpoint(case.run(), &valid(base_request()))
            .await,
        error_code::DURABLE_TASK_STORAGE_UNAVAILABLE,
        "a daemon without checkpoint storage plans nothing",
    );

    let tasks = storage(&fixture);
    let mut named = base_request();
    named.base_id = Some(mint_id().into());
    refused(
        tasks.plan_checkpoint(case.run(), &valid(named)).await,
        error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE,
        "a base capture names no base of its own",
    );
    refused(
        tasks
            .plan_checkpoint(
                case.run(),
                &valid(input(
                    &task,
                    case,
                    CheckpointMode::Incremental,
                    None,
                    &head(),
                )),
            )
            .await,
        error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE,
        "an overlay of a repository commit needs the base it overlays",
    );

    let first = planned(&tasks, case, base_request()).await;
    let first_base = first
        .base_id
        .clone()
        .expect("a base capture allocates its base");
    refused(
        tasks
            .plan_checkpoint(case.run(), &valid(base_request()))
            .await,
        error_code::DURABLE_TASK_STORAGE_UNAVAILABLE,
        "a second capture of the commit waits for the unfinished base",
    );
    refused(
        tasks
            .plan_checkpoint(
                case.run(),
                &valid(input(
                    &task,
                    case,
                    CheckpointMode::Incremental,
                    Some(first_base.clone()),
                    &head(),
                )),
            )
            .await,
        error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE,
        "an overlay never builds on an unfinished base",
    );

    let ready = complete(&tasks, case, &task, &first).await;
    let reused = planned(&tasks, case, base_request()).await;
    assert_eq!(reused.base_id.as_deref(), Some(first_base.as_str()));
    assert_eq!(
        reused.page_ids, first.page_ids,
        "the base's own pages are reused"
    );
    assert_eq!(reused.base_page_ids, first.page_ids);
    assert_eq!(
        reused.manifest_root, ready.manifest_root,
        "and its verified root"
    );
    assert_ne!(reused.checkpoint_id, first.checkpoint_id);
    altered_base(&tasks, case, &task, &first_base, &ready).await;

    refused(
        tasks
            .commit_checkpoint(
                case.run(),
                &mint_id(),
                &valid(CheckpointCommitInput {
                    fencing_token: case.run().fencing_token,
                    durable_task: task.clone(),
                    pin_id: first.pin.id.clone(),
                    manifest_root: "e".repeat(64),
                    history_digest: "e".repeat(64),
                    covered_input_sequence: 0,
                    effect_watermark: 0,
                }),
            )
            .await,
        error_code::DURABLE_TASK_NOT_FOUND,
        "a checkpoint the task never planned is not found",
    );
    fixture.model.finish().await.expect("fixture cleanup");
}

// Value: protects=an API-only objective is captured only as an overlay with no commit;
// fails_when=the API-only target accepts a commit, or a base capture with no commit is
// admitted; why_new=the API-only arms of the plan target and base had no case; seam=the
// production task verbs over live Postgres, the task's target made API-only.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_api_only_durable_task_is_captured_only_as_an_overlay_without_a_commit() {
    exercise(api).await;
}

async fn api(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    api_only(case, &task).await;
    let tasks = storage(&fixture);

    refused(
        tasks
            .plan_checkpoint(
                case.run(),
                &valid(input(
                    &task,
                    case,
                    CheckpointMode::Incremental,
                    None,
                    &head(),
                )),
            )
            .await,
        error_code::DURABLE_TASK_INVALID_TARGET,
        "an API-only task has no commit to name",
    );
    let mut base = incremental(case, &task, 1);
    base.mode = CheckpointMode::RetainBase;
    refused(
        tasks.plan_checkpoint(case.run(), &valid(base)).await,
        error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE,
        "there is no repository commit to retain as a base",
    );

    let overlay = planned(&tasks, case, incremental(case, &task, 1)).await;
    assert_eq!(overlay.mode, CheckpointMode::Incremental);
    assert_eq!(overlay.base_id, None);
    assert_eq!(overlay.base_commit_oid, None);
    assert!(
        overlay.repository.starts_with("api:"),
        "{}",
        overlay.repository
    );
    fixture.model.finish().await.expect("fixture cleanup");
}

/// An overlay on the ready base, refused at commit once the base's frozen page is
/// replaced by another that still matches its own digest.
async fn altered_base(
    tasks: &DurableTasks,
    case: &Case,
    task: &DecisionDurableTaskVersion,
    base: &str,
    ready: &CheckpointPlan,
) {
    let overlay = planned(
        tasks,
        case,
        input(
            task,
            case,
            CheckpointMode::Incremental,
            Some(base.to_owned()),
            &head(),
        ),
    )
    .await;
    assert_eq!(overlay.base_page_ids, ready.page_ids);
    let written = write(tasks, case, task, &overlay, page(0, vec![], vec![])).await;
    let base_page = ready.page_ids.first().expect("the base page");
    let mut replaced = Stored::read(case, Pages::Base, base_page).await;
    let other = page(0, vec![directory("elsewhere")], vec![]);
    replaced.digest = digest_of(&other);
    replaced.body = serde_json::to_vec(&other).expect("page JSON");
    replaced.replace(case, Pages::Base).await;
    refused(
        tasks
            .commit_checkpoint(
                case.run(),
                &id(&overlay.checkpoint_id),
                &valid(CheckpointCommitInput {
                    fencing_token: case.run().fencing_token,
                    durable_task: task.clone(),
                    pin_id: overlay.pin.id.clone(),
                    manifest_root: checkpoint_digest::manifest_root([(
                        written.0.as_str(),
                        written.1.as_str(),
                    )]),
                    history_digest: checkpoint_digest::history_root([]),
                    covered_input_sequence: 0,
                    effect_watermark: 0,
                }),
            )
            .await,
        error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE,
        "the base's pages no longer prove its retained root",
    );
}
