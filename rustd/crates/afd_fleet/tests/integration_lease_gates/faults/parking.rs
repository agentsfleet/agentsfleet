//! Failed approval parking must not retain an otherwise idle fleet claim.

use std::error::Error as _;

use afd_admission::Admissions;
use afd_approval::{Decision, Inbox, Resolution};
use afd_crypto::entropy::Entropy;
use afd_db::config::DbRole;

use super::super::seed::{seed_gate, seed_provider_resolution};
use super::super::*;

const FAIL_STAMP: &str = "CREATE FUNCTION fleet.refuse_park() RETURNS trigger \
    LANGUAGE plpgsql AS $$ BEGIN \
      RAISE EXCEPTION 'injected park failure'; END $$; \
    CREATE TRIGGER refuse_park BEFORE UPDATE OF parked_at ON fleet.admissions \
    FOR EACH ROW WHEN (NEW.parked_at IS NOT NULL) \
    EXECUTE FUNCTION fleet.refuse_park()";
const FAIL_RELEASE: &str = "CREATE FUNCTION fleet.refuse_park() RETURNS trigger \
    LANGUAGE plpgsql AS $$ BEGIN \
      IF EXISTS (SELECT 1 FROM fleet.admissions \
        WHERE fleet_id = NEW.fleet_id AND parked_at IS NOT NULL) THEN \
        RAISE EXCEPTION 'injected park failure'; \
      END IF; RETURN NEW; END $$; \
    CREATE TRIGGER refuse_park BEFORE UPDATE OF lease_expires_at ON runner.fleet_slots \
    FOR EACH ROW EXECUTE FUNCTION fleet.refuse_park()";

// Value: protects=failed park releases claim; fails_when=park error escapes cleanup;
// why_new=successful parks cannot expose rollback; seam=isolated database trigger.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_failed_park_stamp_releases_the_claim_and_retries_after_approval() {
    assert_failed_park_recovers(FAIL_STAMP).await;
}

// Value: protects=partial park rollback; fails_when=slot-write failure retains claim;
// why_new=stamp-write failure precedes partial work; seam=isolated database trigger.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_failed_park_release_rolls_back_the_stamp_and_retries_after_approval() {
    assert_failed_park_recovers(FAIL_RELEASE).await;
}

async fn assert_failed_park_recovers(injection: &'static str) {
    let fixtures = Fixtures::create_isolated_with_queue().await;
    let seeded = ready(&fixtures).await;
    set_config(&fixtures, &seeded.fleet, BUDGETED_CONFIG).await;
    let _default = seed_provider_resolution(&fixtures, &seeded.fleet).await;
    let action = seed_gate(&fixtures, &seeded, "pending").await;
    let claimed = claim(&fixtures, &seeded).await;
    let fence = claimed.fence.as_i64();
    let owner = fixtures.open(DbRole::Migrator).await;
    sqlx::raw_sql(injection)
        .execute(&mut *owner.acquire().await.expect("isolated owner"))
        .await
        .expect("inject selected park failure");

    let failure = fixtures
        .plane()
        .lease_claimed(claimed, &seeded.runner, fixtures.enrolled_at())
        .await
        .expect_err("the park transaction fails");
    assert_original_park_error(&failure);
    assert_eq!(
        admission_state(&fixtures, &seeded.event_id).await,
        (false, false, Some(fence))
    );
    assert!(
        fixtures.slot_is_free(&seeded.fleet).await,
        "a failed park must free its committed claim immediately"
    );

    approve(&fixtures, &seeded, &action).await;
    let retry = claim(&fixtures, &seeded).await;
    assert_eq!(retry.event_id, seeded.event_id);
    assert!(
        retry.fence.as_i64() > fence,
        "retry takes a fresh fenced claim"
    );
    drop(retry);
    owner.close().await;
    fixtures.cleanup().await;
}

fn assert_original_park_error(failure: &afd_fleet::Error) {
    assert_eq!(failure.code(), afd_core::error_code::INTERNAL_DB_QUERY);
    let cause = std::iter::successors(failure.source(), |cause| (*cause).source())
        .find_map(|cause| cause.downcast_ref::<sqlx::Error>())
        .and_then(sqlx::Error::as_database_error)
        .expect("the original SQL cause survives cleanup");
    assert_eq!(cause.code().as_deref(), Some("P0001"));
    assert_eq!(cause.message(), "injected park failure");
}

async fn admission_state(fixtures: &Fixtures, event: &str) -> (bool, bool, Option<i64>) {
    sqlx::query_as(
        "SELECT parked_at IS NOT NULL, settled_at IS NOT NULL, lease_fence \
        FROM fleet.admissions WHERE id = $1::uuid",
    )
    .bind(event)
    .fetch_one(&mut *fixtures.database.acquire().await.expect("observer"))
    .await
    .expect("admission after rollback")
}

async fn approve(fixtures: &Fixtures, seeded: &Ready, action: &str) {
    let resolved = Inbox::new(
        fixtures.database.clone(),
        fixtures.queue().clone(),
        Admissions::new(
            fixtures.database.clone(),
            fixtures.queue().clone(),
            Entropy::new(),
        ),
    )
    .resolve(
        action,
        Decision::Approved,
        "fixture:human",
        "",
        Some(&seeded.fleet),
    )
    .await
    .expect("answer approval after the failed park");
    assert!(matches!(resolved, Resolution::Resolved(_)), "{resolved:?}");
}
