//! Failed and superseded admission settlement cannot charge or end a lease.

use crate::{
    report_commit::{RESPONSE_ACCEPTED, assert_nothing_landed, report},
    report_seed::{Held, held_in},
    rows::Row,
    support::Fixtures,
};
use afd_db::config::DbRole;
use afd_fleet::lease::Committed;

const FAIL_SETTLE: &str = "CREATE FUNCTION fleet.refuse_admission_settlement() RETURNS trigger \
    LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected settlement failure'; END $$; \
    CREATE TRIGGER refuse_admission_settlement BEFORE UPDATE OF settled_at ON fleet.admissions \
    FOR EACH ROW EXECUTE FUNCTION fleet.refuse_admission_settlement()";
const RESTORE_SETTLE: &str = "DROP TRIGGER refuse_admission_settlement ON fleet.admissions; \
    DROP FUNCTION fleet.refuse_admission_settlement()";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn failed_admission_settlement_rolls_back_the_report_and_retry_settles_once() {
    let held = held_in(Fixtures::create_isolated_with_queue().await).await;
    let owner = held.fixtures.open(DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("owner");
    sqlx::raw_sql(FAIL_SETTLE)
        .execute(&mut *connection)
        .await
        .expect("inject fault");
    commit(&held)
        .await
        .expect_err("the settlement trigger refuses the report");
    assert_nothing_landed(&held, "admission settlement failed").await;
    assert!(
        !held.fixtures.admission_settled(&held.event_id).await,
        "a rollback cannot settle the admission"
    );
    assert_eq!(
        held.fixtures
            .text(Row::Lease(held.lease_key.as_str()), "status")
            .await,
        Some(afd_fleet::lease::sql::LEASE_STATUS_ACTIVE.to_owned())
    );
    sqlx::raw_sql(RESTORE_SETTLE)
        .execute(&mut *connection)
        .await
        .expect("remove fault");
    drop(connection);
    assert!(matches!(
        commit(&held).await.expect("retry"),
        Committed::Settled { .. }
    ));
    assert!(held.fixtures.admission_settled(&held.event_id).await);
    assert!(matches!(
        commit(&held).await.expect("repeat"),
        Committed::AlreadySettled
    ));
    assert_eq!(held.fixtures.ledger_rows(&held.event_id).await, 1);
    owner.close().await;
    held.fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_report_cannot_settle_an_admission_with_a_different_fence() {
    let held = held_in(Fixtures::create_isolated_with_queue().await).await;
    let mut connection = held.fixtures.database.acquire().await.expect("database");
    sqlx::query("UPDATE fleet.admissions SET lease_fence = lease_fence + 1 WHERE id = $1::uuid")
        .bind(&held.event_id)
        .execute(&mut *connection)
        .await
        .expect("supersede admission");
    assert!(matches!(
        commit(&held).await.expect("fenced report"),
        Committed::Fenced
    ));
    assert_nothing_landed(&held, "the admission fence changed").await;
    assert!(
        !held.fixtures.admission_settled(&held.event_id).await,
        "a superseded report cannot settle"
    );
    assert_eq!(
        held.fixtures
            .text(Row::Lease(held.lease_key.as_str()), "status")
            .await,
        Some(afd_fleet::lease::sql::LEASE_STATUS_ACTIVE.to_owned())
    );
    sqlx::query("UPDATE fleet.admissions SET lease_fence = $2 WHERE id = $1::uuid")
        .bind(&held.event_id)
        .bind(held.fence.as_i64())
        .execute(&mut *connection)
        .await
        .expect("restore fixture fence");
    drop(connection);
    assert!(matches!(
        commit(&held).await.expect("retry"),
        Committed::Settled { .. }
    ));
    assert!(held.fixtures.admission_settled(&held.event_id).await);
    held.fixtures.cleanup().await;
}

async fn commit(held: &Held) -> afd_fleet::Result<Committed> {
    let leases = held.fixtures.leases_with_dead_queue();
    let lease = leases
        .load_for_report(held.lease_key.as_str(), &held.runner)
        .await?
        .expect("issued lease");
    leases
        .commit_report(report(
            held.lease_key.as_str(),
            &held.runner,
            &lease,
            (&held.lease_key, &held.pricing),
            RESPONSE_ACCEPTED,
            held.now,
        ))
        .await
}
