//! A stale run cannot overwrite a draft's compacted context: once a lapsed
//! setup run's turn is assigned again at a higher fence and the new run saved,
//! the first holder's late save is refused with `UZ-RUN-005`, even when its
//! version would win the compare-and-swap, and the new run's row stays.

use af_context::Checkpoint;
use af_core::error_code;
use afd_setup::setup_run::ContextSave;
use serde_json::Value;

use super::Case;

/// A compacted context for the seat's draft at `generation`, whose notes say
/// who saved it.
fn checkpoint(case: &Case, generation: u64, goal: &str) -> Checkpoint {
    serde_json::from_value(serde_json::json!({
        "format_version": 1,
        "identity": {
            "scope": afd_setup::context::identity(&case.draft.id),
            "model": "setup-model",
            "pinned": "setup-pinned"
        },
        "generation": generation,
        "covered_through": 1,
        "source_digest": "a".repeat(64),
        "summary": {
            "goal": goal, "constraints": [], "decisions": [], "completed": [],
            "remaining": [], "references": []
        }
    }))
    .expect("a valid checkpoint")
}

impl Case {
    /// The fixture host's save of `checkpoint` for run `run_id` at `fence`,
    /// expecting the row at `resource_version`.
    async fn save_context(
        &self,
        run_id: af_core::id::Uuid7,
        fence: u64,
        resource_version: u64,
        checkpoint: Checkpoint,
    ) -> Result<(), afd_setup::Error> {
        self.drafts
            .save_context(
                &self.running.runner,
                ContextSave {
                    run_id,
                    fence,
                    resource_version,
                    checkpoint,
                },
                af_core::clock::now(),
            )
            .await
    }

    /// The draft's context row: its version and document, if it has one.
    async fn context_row(&self) -> Option<(i64, Value)> {
        sqlx::query_as(
            "SELECT resource_version, document FROM memory.context_checkpoints \
             WHERE thread_id = $1",
        )
        .bind(&self.draft.id)
        .fetch_optional(&mut *self.connection().await)
        .await
        .expect("the draft's context row")
    }
}

/// Whether `saved` is refused as a superseded run's save.
fn stale_fenced(saved: &Result<(), afd_setup::Error>) -> bool {
    matches!(saved, Err(refused) if refused.code() == error_code::RUN_STALE_FENCING_TOKEN)
}

// Value: protects=a superseded setup run cannot overwrite the context the live run
// compacted; fails_when=the save keeps only the resource_version compare-and-swap,
// or skips the thread's current fence; why_new=a context save named no run, so a
// late holder whose version matched would win; seam=the production draft assignment
// twice, then Drafts::save_context at each fence over live Postgres.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_superseded_setup_run_cannot_save_context() {
    let case = Case::queued().await;
    let first = case.take_on_host().await;
    assert_eq!(first.fence, 1, "the draft's first run takes fence 1");
    case.lapse(&first.run_id()).await;
    let second = case.take_on_host().await;
    assert_eq!(second.fence, 2, "the lapsed turn is reassigned at fence 2");

    case.save_context(
        second.run_id(),
        second.fence,
        0,
        checkpoint(&case, 1, "fence two"),
    )
    .await
    .expect("the live run saves its context");
    let saved = case.context_row().await.expect("fence 2's row");
    assert_eq!(saved.0, 1, "fence 2 wrote generation 1");

    // The late holder expects the version fence 2 wrote, so the compare-and-swap
    // alone would let it through: the fence is what refuses it.
    let late = case
        .save_context(
            first.run_id(),
            first.fence,
            1,
            checkpoint(&case, 2, "fence one"),
        )
        .await;
    assert!(
        stale_fenced(&late),
        "the first holder's late save: {late:?}"
    );
    let first_save = case
        .save_context(
            first.run_id(),
            first.fence,
            0,
            checkpoint(&case, 1, "fence one"),
        )
        .await;
    assert!(
        stale_fenced(&first_save),
        "the first holder's first save: {first_save:?}"
    );
    let old_fence = case
        .save_context(
            second.run_id(),
            first.fence,
            1,
            checkpoint(&case, 2, "old fence"),
        )
        .await;
    assert!(
        stale_fenced(&old_fence),
        "the live run at the old fence: {old_fence:?}"
    );
    // The fixture's own runtime run is this runner's, but holds no draft.
    let fleet_run = case
        .save_context(
            case.running.run.clone(),
            1,
            1,
            checkpoint(&case, 2, "fleet run"),
        )
        .await;
    assert!(
        stale_fenced(&fleet_run),
        "a fleet run's save: {fleet_run:?}"
    );

    assert_eq!(
        case.context_row().await,
        Some(saved),
        "fence 2's row is unchanged"
    );
    case.cleanup().await;
}
