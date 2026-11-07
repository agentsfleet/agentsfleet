//! A profile read that outlives the claim cannot create issuance side effects.

use afd_admin::test_util::DecisionModelFixture;
use std::time::Duration;

use af_core::{clock, id::Uuid7};
use afd_db::config::DbRole;
use afd_fleet::lease::{Billed, Delivery};
use sqlx::{Connection as _, PgConnection, Postgres, Transaction};

use super::{claim, decision::select, ready};
use crate::{seed, support::Fixtures};

const TEST_TIMEOUT: Duration = Duration::from_secs(5);
const EXPIRY_GRACE: Duration = Duration::from_millis(10);

#[derive(Debug, Default, PartialEq, Eq, sqlx::FromRow)]
struct Issuance {
    leases: i64,
    acquired_events: i64,
    acquisitions: i64,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_profile_read_past_claim_expiry_creates_no_lease_audit_or_tally() {
    let fixtures = Fixtures::create_isolated_with_queue().await;
    let model = DecisionModelFixture::create(&fixtures.database)
        .await
        .expect("reviewed decision model fixture");
    let case = ready(&fixtures).await;
    select(&fixtures, &case.runner, Some(&model.model.id)).await;
    let leases = fixtures.leases();
    let acquired = claim(&fixtures, &case).await;
    assert_eq!(
        leases
            .record_received(&acquired)
            .await
            .expect("received admission")
            .delivery,
        Delivery::First
    );
    let tenant = Uuid7::parse(&case.tenant).expect("native tenant");
    let billed = Billed {
        tenant_id: &tenant,
        posture: seed::POSTURE,
        provider: seed::PROVIDER,
        model: seed::MODEL,
    };
    let migrator = fixtures.open(DbRole::Migrator).await;
    let mut connection = migrator.acquire().await.expect("profile table owner");
    let (held, pid) = block_profile(&mut connection).await;
    let (issued, ()) = tokio::time::timeout(TEST_TIMEOUT, async {
        tokio::join!(
            leases.issue(&case.runner, &acquired, billed, clock::now()),
            expire_while_blocked(&fixtures, held, pid, &case.fleet)
        )
    })
    .await
    .expect("a blocked profile read has a bounded test lifetime");
    assert!(
        issued
            .expect("expiry is a refusal, not a storage error")
            .is_none()
    );
    assert_no_issuance(&fixtures, &case.runner).await;
    select(&fixtures, &case.runner, None).await;
    model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
    drop(connection);
    migrator.close().await;
    drop(leases);
    fixtures.cleanup().await;
}

async fn block_profile(connection: &mut PgConnection) -> (Transaction<'_, Postgres>, i32) {
    let mut held = connection.begin().await.expect("profile blocker");
    let pid = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *held)
        .await
        .expect("blocker identity");
    sqlx::query("LOCK TABLE model.decision_revisions IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *held)
        .await
        .expect("block only this private database");
    (held, pid)
}

/// The issue path touches this table only after locking and reading capacity.
/// Observe that wait before changing the slot; elapsed time alone proves none of it.
async fn expire_while_blocked(
    fixtures: &Fixtures,
    held: Transaction<'_, Postgres>,
    pid: i32,
    fleet: &str,
) {
    let mut probe = fixtures
        .database
        .acquire()
        .await
        .expect("deadline observer");
    loop {
        let blocked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_catalog.pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))")
            .bind(pid).fetch_one(&mut *probe).await.expect("observe blocked profile query");
        if blocked {
            break;
        }
        tokio::task::yield_now().await;
    }
    sqlx::query("UPDATE runner.fleet_slots SET lease_expires_at=date_trunc('milliseconds', clock_timestamp())+$2::interval WHERE fleet_id=$1::uuid")
        .bind(fleet).bind(EXPIRY_GRACE).execute(&mut *probe).await.expect("expire after the issue-time capacity read");
    loop {
        let expired: bool = sqlx::query_scalar("SELECT lease_expires_at <= clock_timestamp() FROM runner.fleet_slots WHERE fleet_id=$1::uuid")
            .bind(fleet).fetch_one(&mut *probe).await.expect("observe actual database expiry");
        if expired {
            break;
        }
        tokio::task::yield_now().await;
    }
    held.rollback()
        .await
        .expect("release the profile read after claim expiry");
}

async fn assert_no_issuance(fixtures: &Fixtures, runner: &Uuid7) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("read issuance effects");
    let found: Issuance = sqlx::query_as(
        "SELECT (SELECT count(*) FROM runner.leases WHERE runner_id=$1::uuid) AS leases, \
         (SELECT count(*) FROM runner.events WHERE runner_id=$1::uuid AND event_type=$2) AS acquired_events, \
         COALESCE((SELECT acquired_count FROM runner.lifetime_counters WHERE runner_id=$1::uuid),0) AS acquisitions")
        .bind(runner.as_str()).bind(afd_runner::sql::event_type::LEASE_ACQUIRED)
        .fetch_one(&mut *connection).await.expect("all native issuance effects");
    assert_eq!(found, Issuance::default());
}
