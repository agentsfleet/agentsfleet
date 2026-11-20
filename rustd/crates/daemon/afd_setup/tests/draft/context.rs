//! The draft's context checkpoint store refuses a superseded run: a save fenced
//! below the draft's thread writes nothing and answers the run plane's
//! stale-fence code, even when its version would win the compare-and-swap.

use super::{
    fixture::{Fixture, now},
    mint,
};
use af_context::{Checkpoint, CheckpointStore as _};
use af_core::{error_code, id::Uuid7};
use afd_setup::context::{Holder, Repository, identity, stored_fence};
use afd_state::sql::{RUN_STATUS_ACTIVE, RUNNER_STATUS_ACTIVE};
use serde_json::{Value, json};

/// A live run of draft `$3` at fence `$8`, on runner `$2`.
const RUN: &str = "INSERT INTO thread.runs \
    (id, runner_id, draft_id, workspace_id, tenant_id, turn_id, turn_created_at, \
     posture, provider, model, last_metered_at, fencing_token, run_expires_at, \
     status, posted_message_count, created_at, updated_at) \
    VALUES ($1,$2,$3,$4,$5,$6,now(),$7,'fixture-provider','fixture-model', \
            now(),$8,now() + interval '1 hour',$9,0,now(),now())";

/// The draft's thread raised to `$2`, as a reassignment raises it.
const FENCE: &str = "UPDATE thread.threads SET fencing_token=$2 WHERE id=$1";

/// The draft's context row: its version and document.
const ROW: &str =
    "SELECT resource_version, document FROM memory.context_checkpoints WHERE draft_id=$1";

/// A compacted context for `draft` at `generation`, whose goal says who saved it.
fn checkpoint(draft: &Uuid7, generation: u64, goal: &str) -> Checkpoint {
    serde_json::from_value(json!({
        "format_version": 1,
        "identity": {
            "scope": identity(draft),
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

/// Seeds the fixture's runner, a live run of `draft` at fences 1 and 2, and
/// raises the draft's thread to fence 2. Answers both runs, oldest first.
async fn superseded(f: &Fixture, draft: &Uuid7) -> [Uuid7; 2] {
    let policy = afd_db::test_util::RunnerPolicy {
        sandbox_tier: "dev_none",
        network_policy: "deny_all_egress",
    };
    let mut connection = f.connection().await;
    afd_db::test_util::RunnerSeed::new(&f.runner, RUNNER_STATUS_ACTIVE, policy)
        .insert(&mut *connection)
        .await
        .expect("the fixture runner");
    let runs = [mint(), mint()];
    for (fence, run) in (1_i64..).zip(&runs) {
        sqlx::query(RUN)
            .bind(run)
            .bind(&f.runner)
            .bind(draft)
            .bind(&f.workspace)
            .bind(&f.tenant)
            .bind(mint())
            .bind(afd_billing::Posture::SelfManaged.as_str())
            .bind(fence)
            .bind(RUN_STATUS_ACTIVE)
            .execute(&mut *connection)
            .await
            .expect("a draft run");
    }
    sqlx::query(FENCE)
        .bind(draft)
        .bind(2_i64)
        .execute(&mut *connection)
        .await
        .expect("the thread at fence 2");
    runs
}

/// The run at `fence` saves `checkpoint` over the row at `expected`.
async fn save(
    f: &Fixture,
    draft: &Uuid7,
    run: &Uuid7,
    fence: u64,
    expected: u64,
    checkpoint: Checkpoint,
) -> af_context::Result<()> {
    let holder = Holder {
        run,
        runner: &f.runner,
        fence,
        active: RUN_STATUS_ACTIVE,
    };
    let mut connection = f.connection().await;
    Repository::fenced(&mut connection, draft.clone(), mint(), holder)
        .commit(expected, checkpoint)
        .await
}

// Value: protects=a superseded setup run cannot overwrite the context the live
// run saved; fails_when=the fenced statements keep only the resource_version
// compare-and-swap, or the store moves without its fence; why_new=the store
// left af_context for afd_setup, and its fence proof moved with it;
// seam=afd_setup::context::Repository over live Postgres.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_save_at_a_superseded_fence_is_refused() {
    let f = Fixture::create().await;
    let created = f
        .drafts
        .create(f.access(), f.request(mint(), None), now())
        .await
        .expect("native create");
    let draft = created.draft.id;
    let [first, second] = superseded(&f, &draft).await;

    save(
        &f,
        &draft,
        &second,
        2,
        0,
        checkpoint(&draft, 1, "fence two"),
    )
    .await
    .expect("the live run saves its context");
    let read = async || {
        sqlx::query_as::<_, (i64, Value)>(ROW)
            .bind(&draft)
            .fetch_one(&mut *f.connection().await)
            .await
            .expect("the draft's context row")
    };
    let saved = read().await;
    assert_eq!(saved.0, 1, "fence 2 wrote generation 1");

    // The late holder expects the version fence 2 wrote, so the compare-and-swap
    // alone would let it through: the fence is what refuses it.
    let late = save(&f, &draft, &first, 1, 1, checkpoint(&draft, 2, "fence one"))
        .await
        .expect_err("the superseded run's save");
    assert!(late.is_stale_fence(), "{late:?}");
    assert_eq!(late.code(), error_code::RUN_STALE_FENCING_TOKEN);
    assert_eq!(read().await, saved, "fence 2's row is unchanged");
    f.cleanup().await;
}

// Value: the fence a save names reaches the statement as the stored bigint; one
// past it names no thread, so it is refused before any write.
#[test]
fn a_fence_no_thread_can_hold_is_refused_as_stale() {
    assert_eq!(stored_fence(2).expect("a storable fence"), 2);
    let past = stored_fence(u64::MAX).expect_err("a fence past bigint");
    assert!(past.is_stale_fence());
    assert_eq!(past.code(), error_code::RUN_STALE_FENCING_TOKEN);
}
