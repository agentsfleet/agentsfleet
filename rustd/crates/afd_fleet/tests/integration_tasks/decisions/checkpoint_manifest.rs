//! Native manifest validation rejects incomplete or escaping state before publication.

use afd_core::{error_code, etag::sha256_hex};
use afd_crypto::checkpoint_digest;
use afd_fleet::task::TaskLifecycle as _;
use afd_wire::task::{
    CheckpointCommitInput, CheckpointEntry, CheckpointFile, CheckpointMode, CheckpointPage,
    CheckpointPageInput,
};

use super::{
    super::fixture::{Case, exercise, id},
    checkpoint::{input, storage, valid},
    fixture::Decisions,
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn native_manifest_rejects_escaping_incomplete_and_rewritten_pages() {
    exercise(manifest).await;
}

async fn manifest(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let tasks = storage(&fixture);
    let mut request = input(
        &task,
        case,
        CheckpointMode::RetainBase,
        None,
        &"0".repeat(40),
    );
    request.entry_count = 3;
    let plan = tasks
        .plan_checkpoint(case.lease(), &valid(request))
        .await
        .expect("plan");
    let manifest = Manifest {
        tasks: &tasks,
        case,
        task: &task,
        plan: &plan,
    };
    manifest.refuse_invalid().await;
    let good = manifest.refuse_mismatches().await;
    let good = manifest.freeze(good).await;
    manifest.commit(good).await;
    fixture.model.finish().await.expect("fixture cleanup");
}

struct Manifest<'a> {
    tasks: &'a afd_fleet::task::Tasks,
    case: &'a Case,
    task: &'a afd_wire::task::DecisionTaskVersion,
    plan: &'a afd_wire::task::CheckpointPlan,
}
impl Manifest<'_> {
    fn page_input(&self, contents: CheckpointPage) -> CheckpointPageInput {
        CheckpointPageInput {
            fencing_token: self.case.lease().fencing_token,
            task: self.task.clone(),
            pin_id: self.plan.pin.id.clone(),
            digest: sha256_hex(&serde_json::to_vec(&contents).expect("page JSON")),
            contents,
        }
    }
    async fn refuse_invalid(&self) {
        let tasks = self.tasks;
        let case = self.case;
        let plan = self.plan;
        let page_id = id(&plan.page_ids[0]);
        let checkpoint = id(&plan.checkpoint_id);
        let page_input = |contents| self.page_input(contents);
        let invalid = invalid_pages();
        for contents in invalid {
            let failure = tasks
                .write_checkpoint_page(
                    case.lease(),
                    &checkpoint,
                    &page_id,
                    &valid(page_input(contents)),
                )
                .await
                .expect_err("unsafe/incomplete manifest cannot be frozen");
            assert_eq!(failure.code(), error_code::TASK_CHECKPOINT_INCOMPATIBLE);
        }
    }
    async fn refuse_mismatches(&self) -> CheckpointPageInput {
        let tasks = self.tasks;
        let case = self.case;
        let plan = self.plan;
        let page_id = id(&plan.page_ids[0]);
        let checkpoint = id(&plan.checkpoint_id);
        let page_input = |contents| self.page_input(contents);
        let contents = safe_page();
        let good = page_input(contents.clone());
        let mut mismatch = good.clone();
        mismatch.digest = "f".repeat(64);
        let failure = tasks
            .write_checkpoint_page(case.lease(), &checkpoint, &page_id, &valid(mismatch))
            .await
            .expect_err("bytes must match digest");
        assert_eq!(failure.code(), error_code::TASK_CHECKPOINT_INCOMPATIBLE);
        let failure = tasks
            .write_checkpoint_page(
                case.lease(),
                &checkpoint,
                &id(&afd_db::test_util::mint_id()),
                &valid(good.clone()),
            )
            .await
            .expect_err("page identity is allocated natively");
        assert_eq!(failure.code(), error_code::TASK_REPLAY_CONFLICT);
        good
    }
    async fn freeze(&self, good: CheckpointPageInput) -> CheckpointPageInput {
        let tasks = self.tasks;
        let case = self.case;
        let plan = self.plan;
        let page_id = id(&plan.page_ids[0]);
        let checkpoint = id(&plan.checkpoint_id);
        let page_input = |contents| self.page_input(contents);
        let contents = good.contents.clone();
        for _ in 0..2 {
            let saved = tasks
                .write_checkpoint_page(case.lease(), &checkpoint, &page_id, &valid(good.clone()))
                .await
                .expect("exact page write/replay");
            assert_eq!(
                serde_json::to_value(saved).expect("saved"),
                serde_json::to_value(&contents).expect("expected")
            );
        }
        let failure = tasks
            .write_checkpoint_page(
                case.lease(),
                &checkpoint,
                &page_id,
                &valid(page_input(page(vec![]))),
            )
            .await
            .expect_err("frozen page cannot be rewritten");
        assert_eq!(failure.code(), error_code::TASK_REPLAY_CONFLICT);
        good
    }
    async fn commit(&self, good: CheckpointPageInput) {
        let tasks = self.tasks;
        let case = self.case;
        let plan = self.plan;
        let page_id = id(&plan.page_ids[0]);
        let checkpoint = id(&plan.checkpoint_id);
        let task = self.task;
        let commit = CheckpointCommitInput {
            fencing_token: case.lease().fencing_token,
            task: task.clone(),
            pin_id: plan.pin.id.clone(),
            manifest_root: checkpoint_digest::manifest_root([(
                page_id.as_str(),
                good.digest.as_str(),
            )]),
            history_digest: checkpoint_digest::history_root([]),
            covered_input_sequence: 0,
            effect_watermark: 0,
        };
        let mut wrong = commit.clone();
        wrong.manifest_root = "e".repeat(64);
        let failure = tasks
            .commit_checkpoint(case.lease(), &checkpoint, &valid(wrong.clone()))
            .await
            .expect_err("root binds exact pages");
        assert_eq!(failure.code(), error_code::TASK_CHECKPOINT_INCOMPATIBLE);
        let saved = tasks
            .commit_checkpoint(case.lease(), &checkpoint, &valid(commit.clone()))
            .await
            .expect("complete generation");
        assert_eq!(
            saved.manifest_root.as_deref(),
            Some(commit.manifest_root.as_str())
        );
        let replay = tasks
            .commit_checkpoint(case.lease(), &checkpoint, &valid(commit))
            .await
            .expect("identical commit replay");
        assert!(replay.replayed);
        let failure = tasks
            .commit_checkpoint(case.lease(), &checkpoint, &valid(wrong))
            .await
            .expect_err("committed root immutable");
        assert_eq!(failure.code(), error_code::TASK_REPLAY_CONFLICT);
    }
}

fn page(entries: Vec<CheckpointEntry>) -> CheckpointPage {
    CheckpointPage {
        page_number: 0,
        entries,
        history_objects: vec![],
    }
}

fn invalid_pages() -> [CheckpointPage; 5] {
    [
        page(vec![
            CheckpointEntry::Directory { path: "z".into() },
            CheckpointEntry::Directory { path: "a".into() },
        ]),
        page(vec![CheckpointEntry::Symlink {
            path: "link".into(),
            target: "../escape".into(),
        }]),
        page(vec![CheckpointEntry::Remove {
            path: "removed".into(),
        }]),
        page(vec![CheckpointEntry::File(CheckpointFile {
            path: "file".into(),
            executable: false,
            size: 1,
            objects: vec![],
        })]),
        CheckpointPage {
            page_number: 0,
            entries: vec![],
            history_objects: vec![afd_db::test_util::mint_id()],
        },
    ]
}

fn safe_page() -> CheckpointPage {
    page(vec![
        CheckpointEntry::Directory { path: "a".into() },
        CheckpointEntry::File(CheckpointFile {
            path: "a/empty".into(),
            executable: true,
            size: 0,
            objects: vec![],
        }),
        CheckpointEntry::Symlink {
            path: "a/link".into(),
            target: "../a/empty".into(),
        },
    ])
}
