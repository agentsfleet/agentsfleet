//! A fleet chat's summary rides the draft's store, keyed by the fleet's
//! thread: the daemon's context verb saves it for the live run of the
//! fleet, and refuses a superseded run's save as stale-fenced, even when
//! its version would win the compare-and-swap.

use super::{
    fixture::{Fixture, now},
    mint,
};
use af_context::Checkpoint;
use af_core::{error_code, id::Uuid7};
use afd_db::test_util::FleetSeed;
use afd_setup::context::fleet_identity;
use afd_setup::setup_run::ContextSave;
use afd_state::sql::{FLEET_STATUS_ACTIVE, RUN_STATUS_ACTIVE, RUNNER_STATUS_ACTIVE};
use serde_json::{Value, json};

/// A live run of fleet `$3` at fence `$8`, on runner `$2`.
const RUN: &str = "INSERT INTO thread.runs \
    (id, runner_id, fleet_id, workspace_id, tenant_id, turn_id, turn_created_at, \
     posture, provider, model, last_metered_at, fencing_token, run_expires_at, \
     status, posted_message_count, created_at, updated_at) \
    VALUES ($1,$2,$3,$4,$5,$6,now(),$7,'fixture-provider','fixture-model', \
            now(),$8,now() + interval '1 hour',$9,0,now(),now())";

/// The fleet's thread raised to `$2`, as a reassignment raises it.
const FENCE: &str = "UPDATE thread.threads SET fencing_token=$2 WHERE id=$1";

/// The fleet's context row: its version and document.
const ROW: &str =
    "SELECT resource_version, document FROM memory.context_checkpoints WHERE thread_id=$1";

/// A fleet chat's summary at `generation`, whose goal says who saved it.
fn checkpoint(fleet: &Uuid7, generation: u64, goal: &str) -> Checkpoint {
    serde_json::from_value(json!({
        "format_version": 1,
        "identity": {
            "scope": fleet_identity(fleet),
            "model": "fleet-model",
            "pinned": "fleet-pinned"
        },
        "generation": generation,
        "covered_through": 3,
        "source_digest": "a".repeat(64),
        "summary": {
            "goal": goal, "constraints": [], "decisions": [], "completed": [],
            "remaining": [], "references": []
        }
    }))
    .expect("a valid checkpoint")
}

/// Seeds a fleet in the fixture's workspace, the fixture's runner, a live run
/// of the fleet at fences 1 and 2, and raises the fleet's thread to fence 2.
/// Answers the fleet and both runs, oldest first.
async fn superseded(f: &Fixture) -> (Uuid7, [Uuid7; 2]) {
    let fleet = mint();
    let policy = afd_db::test_util::RunnerPolicy {
        sandbox_tier: "dev_none",
        network_policy: "deny_all_egress",
    };
    let mut connection = f.connection().await;
    FleetSeed::new(&fleet, &f.workspace, &f.tenant, FLEET_STATUS_ACTIVE)
        .insert(&mut *connection)
        .await
        .expect("the fixture fleet and its thread");
    afd_db::test_util::RunnerSeed::new(&f.runner, RUNNER_STATUS_ACTIVE, policy)
        .insert(&mut *connection)
        .await
        .expect("the fixture runner");
    let runs = [mint(), mint()];
    for (fence, run) in (1_i64..).zip(&runs) {
        sqlx::query(RUN)
            .bind(run)
            .bind(&f.runner)
            .bind(&fleet)
            .bind(&f.workspace)
            .bind(&f.tenant)
            .bind(mint())
            .bind(afd_billing::Posture::SelfManaged.as_str())
            .bind(fence)
            .bind(RUN_STATUS_ACTIVE)
            .execute(&mut *connection)
            .await
            .expect("a fleet run");
    }
    sqlx::query(FENCE)
        .bind(&fleet)
        .bind(2_i64)
        .execute(&mut *connection)
        .await
        .expect("the thread at fence 2");
    (fleet, runs)
}

/// The run `run` at `fence` saves `checkpoint` over the row at `expected`,
/// through the daemon's context verb.
async fn save(
    f: &Fixture,
    run: &Uuid7,
    fence: u64,
    expected: u64,
    checkpoint: Checkpoint,
) -> Result<(), afd_setup::Error> {
    let save = ContextSave {
        run_id: run.clone(),
        fence,
        resource_version: expected,
        checkpoint,
    };
    f.drafts.save_context(&f.runner, save, now()).await
}

// Value: protects=a superseded fleet run cannot overwrite the summary the live
// run saved; fails_when=the fenced statements name only a draft's thread, or the
// store moves without its fence; why_new=a fleet chat's summary now rides the
// draft's store; seam=Drafts::save_context over live Postgres.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_fleet_summary_at_a_superseded_fence_is_refused() {
    let f = Fixture::create().await;
    let (fleet, [first, second]) = superseded(&f).await;

    save(&f, &second, 2, 0, checkpoint(&fleet, 1, "fence two"))
        .await
        .expect("the live run saves its fleet's summary");
    let read = async || {
        sqlx::query_as::<_, (i64, Value)>(ROW)
            .bind(&fleet)
            .fetch_one(&mut *f.connection().await)
            .await
            .expect("the fleet's context row")
    };
    let saved = read().await;
    assert_eq!(saved.0, 1, "fence 2 wrote generation 1");

    // The late holder expects the version fence 2 wrote, so the compare-and-swap
    // alone would let it through: the fence is what refuses it.
    let late = save(&f, &first, 1, 1, checkpoint(&fleet, 2, "fence one"))
        .await
        .expect_err("the superseded run's save");
    assert_eq!(late.code(), error_code::RUN_STALE_FENCING_TOKEN, "{late:?}");
    assert_eq!(read().await, saved, "fence 2's row is unchanged");

    // A summary written under the draft scope is not the fleet's next one.
    let foreign = save(&f, &second, 2, 1, {
        let mut misnamed = checkpoint(&fleet, 2, "misnamed");
        misnamed.identity.scope = afd_setup::context::identity(&fleet);
        misnamed
    })
    .await
    .expect_err("a save under another scope");
    assert_ne!(foreign.code(), error_code::RUN_STALE_FENCING_TOKEN);
    assert_eq!(read().await, saved, "the misnamed save wrote nothing");
    f.cleanup().await;
}
