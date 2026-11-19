//! Where a steer went, read back through `Steer::append` over live Postgres.
//!
//! The ledger's one statement reports the facts: whether a run of the fleet is
//! live, which message began it, and what waits ahead. These seed a live run by
//! hand, the way a runner running a message leaves it, and prove each answer
//! end to end, a retry's included: it is answered from the ledger as it stands
//! now, and the message that began the live run is `started` however often it
//! is sent.
//!
//! Marked `#[ignore]` so `make test-unit-all` still compiles and lints this
//! without a datastore; `make test-integration-rustd` runs it.

use af_core::id::Uuid7;
use std::error::Error;

use af_core::turn::status;
use afd_db::test_util::{RunnerPolicy, RunnerSeed, mint_id};
use afd_turns::view::disposition::{Placed, SteerDisposition};
use afd_turns::view::{Steer, Steered, steer_actor};

use crate::integration_steer_retry::REQUEST_JSON;
use crate::support::TurnsLane;

type Outcome = Result<(), Box<dyn Error>>;

/// The person every message in this suite comes from.
const PERSON: &str = "user_2folder";

/// One operation id, so a message can be sent again.
const OPERATION: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0e01";

/// The assignment the seeded runner carries.
const POLICY: RunnerPolicy<'static> = RunnerPolicy {
    sandbox_tier: "dev_none",
    network_policy: "deny_all_egress",
};

const INSERT_RUN: &str = "\
INSERT INTO thread.runs
  (id, runner_id, fleet_id, workspace_id, tenant_id, turn_id, turn_created_at,
   posture, provider, model, last_metered_at, fencing_token, run_expires_at, status,
   posted_message_count, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, now(),
        'platform', 'test', 'test', now(), 1, now() + interval '1 hour', $7, 0, now(), now())";

/// Opens a waiting turn as the claim does when a run takes it.
const OPEN_TURN: &str =
    "UPDATE thread.turns SET opened_at = now(), status = $2, updated_at = now() WHERE id = $1";

const INSERT_SLOT: &str = "\
INSERT INTO thread.threads
  (id, fleet_id, last_runner_id, fencing_token, run_expires_at, created_at, updated_at)
VALUES ($1, $1, $2, 1, now() + interval '1 hour', now(), now())
ON CONFLICT (id) DO UPDATE SET last_runner_id = EXCLUDED.last_runner_id, fencing_token = 1,
    run_expires_at = EXCLUDED.run_expires_at, updated_at = now()";

/// Sends the person's message, keyed by `operation` when it has one.
async fn send(
    steer: &Steer,
    lane: &TurnsLane,
    operation: Option<&str>,
) -> Result<Steered, Box<dyn Error>> {
    let actor = steer_actor(PERSON);
    let steered = steer
        .append(
            &lane.fleet,
            &lane.workspace,
            &actor,
            REQUEST_JSON,
            operation,
        )
        .await?;
    Ok(steered)
}

/// The placement a send answers: `joined` is the run it folded into, if any.
fn placed(disposition: SteerDisposition, joined: Option<&str>) -> Placed {
    Placed {
        disposition,
        folded_into_turn_id: joined.map(str::to_owned),
    }
}

/// Makes the run `head` began the fleet's live run: a runner, the event the
/// run serves, an active run on it and the fleet's slot held under it.
async fn go_live(lane: &TurnsLane, head: &str) -> Outcome {
    let head = Uuid7::parse(head)?;
    let runner = mint_id();
    let mut connection = lane.connection().await;
    RunnerSeed::new(&runner, afd_state::sql::RUNNER_STATUS_ACTIVE, POLICY)
        .insert(&mut *connection)
        .await?;
    // The send wrote the head's turn; a claim opens it, as here.
    sqlx::query(OPEN_TURN)
        .bind(&head)
        .bind(status::RECEIVED)
        .execute(&mut *connection)
        .await?;
    sqlx::query(INSERT_RUN)
        .bind(mint_id())
        .bind(&runner)
        .bind(&lane.fleet)
        .bind(&lane.workspace)
        .bind(&lane.tenant)
        .bind(&head)
        .bind(afd_state::sql::RUN_STATUS_ACTIVE)
        .execute(&mut *connection)
        .await?;
    sqlx::query(INSERT_SLOT)
        .bind(&lane.fleet)
        .bind(&runner)
        .execute(&mut *connection)
        .await?;
    Ok(())
}

/// Parks `admission`, so a send behind it must wait for it.
async fn park(lane: &TurnsLane, admission: &str) -> Outcome {
    let admission = Uuid7::parse(admission)?;
    sqlx::query("UPDATE thread.turns SET parked_at = now() WHERE id = $1")
        .bind(&admission)
        .execute(&mut *lane.connection().await)
        .await?;
    Ok(())
}

/// With nothing running and nothing waiting, a message starts a run.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_send_to_a_fleet_with_nothing_running_starts_a_run() -> Outcome {
    let lane = TurnsLane::open().await;
    let started = send(&Steer::new(lane.admissions()), &lane, None).await?;
    assert_eq!(started.placed, placed(SteerDisposition::Started, None));
    assert!(!started.replayed());
    lane.cleanup().await;
    Ok(())
}

/// A message sent while a run is live joins it, and the answer names the run.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_send_behind_a_live_run_folds_into_it() -> Outcome {
    let lane = TurnsLane::open().await;
    let steer = Steer::new(lane.admissions());
    let head = send(&steer, &lane, None).await?;
    go_live(&lane, &head.turn_id).await?;

    let folded = send(&steer, &lane, None).await?;
    assert_ne!(folded.turn_id, head.turn_id);
    assert_eq!(
        folded.placed,
        placed(SteerDisposition::Folded, Some(&head.turn_id))
    );
    lane.cleanup().await;
    Ok(())
}

/// A parked message the run must not pass makes every later send wait, even
/// while the run is live.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_send_behind_a_parked_message_waits_its_turn() -> Outcome {
    let lane = TurnsLane::open().await;
    let steer = Steer::new(lane.admissions());
    let head = send(&steer, &lane, None).await?;
    go_live(&lane, &head.turn_id).await?;
    let gated = send(&steer, &lane, None).await?;
    park(&lane, &gated.turn_id).await?;

    let behind = send(&steer, &lane, None).await?;
    assert_eq!(behind.placed, placed(SteerDisposition::Queued, None));
    lane.cleanup().await;
    Ok(())
}

/// A retry of the message that began the live run is answered as the first
/// send was, `started`. It never folds into the run it began.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_retry_of_the_send_that_started_the_run_is_still_started() -> Outcome {
    let lane = TurnsLane::open().await;
    let steer = Steer::new(lane.admissions());
    let first = send(&steer, &lane, Some(OPERATION)).await?;
    assert_eq!(first.placed, placed(SteerDisposition::Started, None));
    go_live(&lane, &first.turn_id).await?;

    let retried = send(&steer, &lane, Some(OPERATION)).await?;
    assert!(retried.replayed());
    assert_eq!(retried.turn_id, first.turn_id);
    assert_eq!(retried.placed, placed(SteerDisposition::Started, None));
    lane.cleanup().await;
    Ok(())
}

/// A retry of a message that folded names the same run the first answer did.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_retry_of_a_folded_send_stays_folded() -> Outcome {
    let lane = TurnsLane::open().await;
    let steer = Steer::new(lane.admissions());
    let head = send(&steer, &lane, None).await?;
    go_live(&lane, &head.turn_id).await?;
    let first = send(&steer, &lane, Some(OPERATION)).await?;
    let joined = placed(SteerDisposition::Folded, Some(&head.turn_id));
    assert_eq!(first.placed, joined);

    let retried = send(&steer, &lane, Some(OPERATION)).await?;
    assert!(retried.replayed());
    assert_eq!(retried.turn_id, first.turn_id);
    assert_eq!(retried.placed, joined);
    lane.cleanup().await;
    Ok(())
}
