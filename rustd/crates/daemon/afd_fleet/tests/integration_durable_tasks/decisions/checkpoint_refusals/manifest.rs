//! A commit re-reads every frozen page: paths must ascend across pages, and a page
//! altered at rest after it was frozen is never published.

use af_core::error_code;
use af_crypto::checkpoint_digest;
use af_wire::durable_task::{CheckpointCommitInput, CheckpointPlan, DecisionDurableTaskVersion};
use afd_fleet::durable_task::{DurableTaskLifecycle as _, DurableTasks};

use super::{
    super::{
        super::fixture::{Case, exercise, id},
        checkpoint::{storage, valid},
        fixture::Decisions,
    },
    Pages, Stored, api_only, digest_of, directory, incremental, lapse, page, planned, refused,
    write,
};

// Value: protects=a manifest publishes only when its input cursor is the task's and its
// pages, read back at commit, still ascend and still match what was frozen;
// fails_when=the input-cursor check, the cross-page order check, the
// stored-digest check, the history-identity parse, the page-number check or the
// page-count check is dropped from the commit's page scan; why_new=the commit's scan
// refusals had no case; seam=the production task verbs over live Postgres, frozen rows
// replaced as the runtime role may replace them.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_commit_refuses_pages_out_of_order_or_altered_after_they_were_frozen() {
    exercise(manifest).await;
}

async fn manifest(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    api_only(case, &task).await;
    let tasks = storage(&fixture);

    let disordered = two_pages(&tasks, case, &task).await;
    let pages = [
        write(
            &tasks,
            case,
            &task,
            &disordered,
            page(0, vec![directory("b")], vec![]),
        )
        .await,
        write(
            &tasks,
            case,
            &task,
            &disordered,
            page(1, vec![directory("a")], vec![]),
        )
        .await,
    ];
    refused(
        tasks
            .commit_checkpoint(
                case.run(),
                &id(&disordered.checkpoint_id),
                &commit(case, &task, &disordered, &pages),
            )
            .await,
        error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE,
        "each page ascends alone, but the manifest does not",
    );

    lapse(case, &disordered).await;
    let plan = two_pages(&tasks, case, &task).await;
    let pages = [
        write(
            &tasks,
            case,
            &task,
            &plan,
            page(0, vec![directory("a")], vec![]),
        )
        .await,
        write(
            &tasks,
            case,
            &task,
            &plan,
            page(1, vec![directory("b")], vec![]),
        )
        .await,
    ];
    let mut ahead = commit(case, &task, &plan, &pages).into_inner();
    ahead.covered_input_sequence = 1;
    refused(
        tasks
            .commit_checkpoint(case.run(), &id(&plan.checkpoint_id), &valid(ahead))
            .await,
        error_code::DURABLE_TASK_STALE_STATE,
        "a commit claiming inputs the task has not covered",
    );
    altered(&tasks, case, &task, &plan, &pages).await;
    let saved = tasks
        .commit_checkpoint(
            case.run(),
            &id(&plan.checkpoint_id),
            &commit(case, &task, &plan, &pages),
        )
        .await
        .expect("the restored manifest publishes");
    assert_eq!(
        saved.manifest_root.as_deref(),
        Some(commit(case, &task, &plan, &pages).manifest_root.as_str())
    );
    fixture.model.finish().await.expect("fixture cleanup");
}

/// A two-page overlay of two directories.
async fn two_pages(
    tasks: &DurableTasks,
    case: &Case,
    task: &DecisionDurableTaskVersion,
) -> CheckpointPlan {
    let mut request = incremental(case, task, 2);
    request.entry_count = 2;
    planned(tasks, case, request).await
}

fn commit(
    case: &Case,
    task: &DecisionDurableTaskVersion,
    plan: &CheckpointPlan,
    pages: &[(String, String)],
) -> garde::Valid<CheckpointCommitInput> {
    valid(CheckpointCommitInput {
        fencing_token: case.run().fencing_token,
        durable_task: task.clone(),
        pin_id: plan.pin.id.clone(),
        manifest_root: checkpoint_digest::manifest_root(
            pages
                .iter()
                .map(|(page, digest)| (page.as_str(), digest.as_str())),
        ),
        history_digest: checkpoint_digest::history_root([]),
        covered_input_sequence: 0,
        effect_watermark: 0,
    })
}

async fn altered(
    tasks: &DurableTasks,
    case: &Case,
    task: &DecisionDurableTaskVersion,
    plan: &CheckpointPlan,
    pages: &[(String, String)],
) {
    let (second, _digest) = pages.get(1).expect("the second page");
    let original = Stored::read(case, Pages::Overlay, second).await;

    let mut rewritten = original.clone();
    rewritten.body = serde_json::to_vec(&page(1, vec![directory("c")], vec![])).expect("page JSON");
    let mut renamed = original.clone();
    let bad_history = page(1, vec![directory("b")], vec!["not-an-object".into()]);
    renamed.digest = digest_of(&bad_history);
    renamed.body = serde_json::to_vec(&bad_history).expect("page JSON");
    let mut moved = original.clone();
    moved.page_number = 5;
    for (alteration, why) in [
        (rewritten, "bytes that no longer match the frozen digest"),
        (renamed, "a history object that names no object"),
        (moved, "a page out of its allocated position"),
    ] {
        alteration.replace(case, Pages::Overlay).await;
        refused(
            tasks
                .commit_checkpoint(
                    case.run(),
                    &id(&plan.checkpoint_id),
                    &commit(case, task, plan, pages),
                )
                .await,
            error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE,
            why,
        );
    }

    Stored::delete(case, &original.id).await;
    refused(
        tasks
            .commit_checkpoint(
                case.run(),
                &id(&plan.checkpoint_id),
                &commit(case, task, plan, pages),
            )
            .await,
        error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE,
        "a manifest missing one of its pages",
    );
    original.replace_missing(case, Pages::Overlay).await;
}
