//! Activation decides under its own locks: setup that waits past its budget,
//! a readiness lost while it waited, and an activation update that matched
//! nothing all leave the fleet installing for the same key to finish.

use super::super::{fixture::now, mint, refusal::Refusal};
use super::fixture::{Case, check};
use af_core::{error_code, id::Uuid7, test_util::trace::Capture};
use afd_setup::{SavedFleet, types::Start};
use sqlx::Connection as _;

/// Holds the fleet row the way activation's write locks it
/// (`creation/sql/lock.sql`). Setup's readiness reads take no lock, so a held
/// row stops activation only where it writes: at the stream record, or at the
/// locked recheck once that record is in.
const HOLD_FLEET: &str = "SELECT id FROM fleet.fleets WHERE id=$1 FOR UPDATE";

/// The stream record an earlier pass left, so the retry's first write is the
/// locked recheck.
const STREAM_READY: &str = "UPDATE fleet.fleets SET setup_stream_ready=true WHERE id=$1";

/// The holding session, so a proof can wait for work to queue behind it.
const BACKEND: &str = "SELECT pg_backend_pid()";

/// Saves `name` unfunded, so setup stops before the stream and activation,
/// then funds the wallet: the next Save under the key reaches activation.
async fn funded_retained(c: &Case, name: &str) -> (Uuid7, afd_setup::types::SaveFleet, Uuid7) {
    let (_, request) = c.request(name, Start::None).await;
    let key = mint();
    let first = c.save(&key, request.clone()).await;
    assert_eq!(first.status, "installing");
    c.fund().await;
    (key, request, first.id)
}

fn installing(saved: &SavedFleet) {
    assert_eq!(saved.status, "installing");
    assert_eq!(check(saved, "activation"), "pending");
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn setup_waiting_past_its_budget_answers_installing_and_the_key_finishes_it() {
    // Its own database: activation waits holding a lock on `fleet.fleets`, and a
    // table lock another proof queues there would stall every session behind it.
    let c = Case::isolated().await;
    let (key, request, fleet) = funded_retained(&c, "budget-elapsed").await;
    let mut holder = c.base.connection().await;
    let mut held = holder.begin().await.expect("holder transaction");
    sqlx::query(HOLD_FLEET)
        .bind(&fleet)
        .execute(&mut *held)
        .await
        .expect("hold the fleet row");
    let capture = Capture::install();
    let waited = c.save(&key, request.clone()).await;
    let elapsed = capture.only("fleet_setup_budget_elapsed");
    drop(capture);
    assert_eq!(waited.id, fleet);
    installing(&waited);
    assert_eq!(elapsed.field("fleet_id"), Some(fleet.as_str()));
    assert_eq!(
        elapsed.field("error_code"),
        Some(error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
    held.rollback().await.expect("release the fleet row");
    drop(holder);
    assert_eq!(c.save(&key, request).await.status, "active");
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_stream_record_lost_while_activation_waited_holds_the_fleet_installing() {
    // Its own database: activation waits holding a lock on `fleet.fleets`, and a
    // table lock another proof queues there would stall every session behind it.
    let c = Case::isolated().await;
    let (key, request, fleet) = funded_retained(&c, "readiness-lost").await;
    sqlx::query(STREAM_READY)
        .bind(&fleet)
        .execute(&mut *c.base.connection().await)
        .await
        .expect("an earlier pass recorded the stream");
    let mut holder = c.base.connection().await;
    let mut held = holder.begin().await.expect("holder transaction");
    let pid: i32 = sqlx::query_scalar(BACKEND)
        .fetch_one(&mut *held)
        .await
        .expect("holder session");
    sqlx::query(HOLD_FLEET)
        .bind(&fleet)
        .execute(&mut *held)
        .await
        .expect("hold the fleet row");
    let (waited, ()) = tokio::join!(c.save(&key, request.clone()), async {
        c.base.waits_on(pid, 1).await;
        sqlx::query("UPDATE fleet.fleets SET setup_stream_ready=false WHERE id=$1")
            .bind(&fleet)
            .execute(&mut *held)
            .await
            .expect("lose the stream record");
        held.commit().await.expect("release the fleet row");
    });
    assert_eq!(waited.id, fleet);
    installing(&waited);
    assert_eq!(
        check(&waited, "stream"),
        "pending",
        "rechecked under the lock"
    );
    drop(holder);
    assert_eq!(c.save(&key, request).await.status, "active");
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_activation_update_that_matches_nothing_rolls_back_as_setup_changed() {
    let c = Case::create().await;
    let (key, request, fleet) = funded_retained(&c, "activation-skipped").await;
    let refusal = Refusal::install(
        &c.base.owner_url(),
        "fleet.fleets",
        "skip_activate",
        fleet.as_str(),
        &format!("NEW.id='{fleet}'::uuid AND NEW.status='active'"),
        "RETURN NULL",
    )
    .await
    .expect("the fixture refusal installs");
    let refused = c
        .creations
        .save(c.base.access(), &key, request.clone(), now())
        .await
        .expect_err("the guarded update matched no row");
    assert_eq!(refused.current_state(), Some("setup_changed"));
    let stalled = c.creations.read(c.base.access(), &fleet).await;
    installing(&stalled.expect("retained fleet"));
    drop(refusal);
    assert_eq!(c.save(&key, request).await.status, "active");
    c.cleanup().await;
}
