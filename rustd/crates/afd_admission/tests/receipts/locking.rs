//! The fleet row lock: a revision install cannot slip under an admission, and
//! staging admits a fleet in any status (`M242_001` Dimensions 2.5 and 3.4).

use std::time::Duration;

use afd_admission::Reply as ReplyTo;
use afd_db::test_util::mint_id;
use sqlx::Connection as _;

use super::{lane::Lane, ledger::steer, queue::over_fake};

/// How long an admission may take to start waiting on the held fleet lock.
const BLOCK_WAIT: Duration = Duration::from_secs(5);

/// Sessions waiting on a backend, read from a separate autocommit connection.
const BLOCKED_ON: &str =
    "SELECT count(*) FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid))";

/// A copy of the fleet's head revision, stamped with a policy this build does not run.
const FOREIGN_REVISION: &str = "INSERT INTO fleet.revisions \
    (id, fleet_id, revision, definition, canonical_digest, policy_semantics, runtime_model_ref, created_at) \
    SELECT $2::uuid, r.fleet_id, r.revision + 1, r.definition, md5(random()::text), 'foreign-semantics', \
           r.runtime_model_ref, now() \
    FROM fleet.revisions r JOIN fleet.fleets f ON f.head_revision_id = r.id WHERE f.id = $1::uuid";

async fn wait_until_blocked(lane: &Lane, holder: i32) {
    let deadline = tokio::time::Instant::now() + BLOCK_WAIT;
    loop {
        let blocked: i64 = sqlx::query_scalar(BLOCKED_ON)
            .bind(holder)
            .fetch_one(&mut *lane.database.acquire().await.expect("observer"))
            .await
            .expect("blocked sessions");
        if blocked > 0 {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the admission never waited"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_admission_waiting_on_a_revision_install_never_lands_under_the_old_policy() {
    let lane = std::sync::Arc::new(Lane::seed().await);
    let (_server, admissions) = over_fake(&lane, &[]).await;
    let mut connection = lane.database.acquire().await.expect("installer");
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *connection)
        .await
        .expect("installer backend");
    let mut install = connection.begin().await.expect("open the install");
    sqlx::query("SELECT 1 FROM fleet.fleets WHERE id = $1::uuid FOR UPDATE")
        .bind(&lane.fleet)
        .execute(&mut *install)
        .await
        .expect("the install holds the fleet row");
    let revision = mint_id();
    sqlx::query(FOREIGN_REVISION)
        .bind(&lane.fleet)
        .bind(&revision)
        .execute(&mut *install)
        .await
        .expect("a foreign-policy revision");
    sqlx::query("UPDATE fleet.fleets SET head_revision_id = $2::uuid WHERE id = $1::uuid")
        .bind(&lane.fleet)
        .bind(&revision)
        .execute(&mut *install)
        .await
        .expect("the new revision becomes head");
    let waiter = tokio::spawn({
        let (lane, admissions) = (std::sync::Arc::clone(&lane), admissions.clone());
        async move {
            let key = mint_id();
            admissions.admit(steer(&lane, &key, ReplyTo::None)).await
        }
    });
    wait_until_blocked(&lane, holder).await;
    install.commit().await.expect("the install commits");
    drop(connection);
    let refused = waiter
        .await
        .expect("the admission task")
        .expect_err("the admission meets the new head");
    assert_eq!(
        refused.code(),
        afd_core::error_code::AUTHORING_POLICY,
        "{refused}"
    );
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fleet.admissions WHERE fleet_id = $1::uuid")
            .bind(&lane.fleet)
            .fetch_one(&mut *lane.database.acquire().await.expect("observer"))
            .await
            .expect("rows");
    assert_eq!(rows, 0, "nothing lands under the old policy");
    std::sync::Arc::into_inner(lane)
        .expect("the admission task released the lane")
        .cleanup()
        .await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn staging_admits_on_an_installing_fleet() {
    let lane = Lane::seed().await;
    let (_server, admissions) = over_fake(&lane, &[]).await;
    sqlx::query("UPDATE fleet.fleets SET status = 'installing' WHERE id = $1::uuid")
        .bind(&lane.fleet)
        .execute(&mut *lane.database.acquire().await.expect("fixture"))
        .await
        .expect("the fleet is installing");
    let key = mint_id();
    let refused = admissions
        .admit_runnable(steer(&lane, &key, ReplyTo::None))
        .await
        .expect_err("an installing fleet takes no runnable send");
    assert_eq!(
        refused.not_runnable(),
        Some(afd_fleet_lifecycle::FleetStatus::Installing)
    );
    let staged_key = mint_id();
    let prepared = admissions
        .prepare(steer(&lane, &staged_key, ReplyTo::None))
        .await
        .expect("prepare");
    let mut connection = lane.database.acquire().await.expect("activation");
    let mut tx = connection.begin().await.expect("begin");
    let staged = admissions
        .stage(&mut tx, prepared)
        .await
        .expect("stage admits");
    tx.commit().await.expect("commit");
    assert!(!staged.replayed);
    drop(connection);
    lane.cleanup().await;
}
