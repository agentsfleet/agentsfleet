//! A delete is refused, changing nothing, while its fleet holds a schedule, and
//! the check that runs before the scheduler is asked refuses what the delete
//! would.

use af_core::error_code;
use af_core::id::Uuid7;

use super::fixture::{Fixture, valid};
use super::mint;

/// A schedule row on `fleet`, as a create leaves one the scheduler holds.
async fn schedule_on(f: &Fixture, fleet: &Uuid7) {
    let mut connection = f.connection().await;
    sqlx::query(
        "INSERT INTO fleet.schedules \
           (id, fleet_id, source, provider_schedule_id, cron_expression, timezone, \
            message, once, desired_status, sync_status, resource_version, \
            created_at, updated_at) \
         VALUES ($1, $2, 'api', 'nightly', '0 3 * * *', 'UTC', 'run the nightly repair', \
                 false, 'active', 'synced', 1, now(), now())",
    )
    .bind(mint())
    .bind(fleet)
    .execute(&mut *connection)
    .await
    .expect("a schedule row seeds");
}

// Value: protects=the cascade never takes a schedule the scheduler may still
// hold; fails_when=the purge deletes a fleet that holds a schedule;
// seam=`Creations::delete` over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_delete_of_a_fleet_holding_a_schedule_is_refused_and_changes_nothing() {
    let f = Fixture::create().await;
    let created = f
        .creations
        .create(
            f.access(),
            &valid(f.request(&mint())),
            super::fixture::now(),
        )
        .await
        .expect("the fleet is created");
    schedule_on(&f, &created.fleet.id).await;

    let refused = f
        .creations
        .delete(f.access(), &created.fleet.id, &created.fleet.etag)
        .await
        .expect_err("a fleet still holding a schedule must not go");

    assert_eq!(refused.code(), error_code::SCHEDULE_SYNCING);
    assert_eq!(refused.current_state(), Some("syncing"));
    assert_eq!(
        f.counts().await,
        (1, 1, 1),
        "the fleet, thread and turn stay"
    );
    f.cleanup().await;
}

// Value: protects=a stale tag is refused before a delete removes any schedule
// upstream; fails_when=the check passes a tag the delete would refuse;
// seam=`Creations::confirm_delete` over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_check_before_a_delete_refuses_a_stale_tag_and_passes_the_current_one() {
    let f = Fixture::create().await;
    let created = f
        .creations
        .create(
            f.access(),
            &valid(f.request(&mint())),
            super::fixture::now(),
        )
        .await
        .expect("the fleet is created");
    let stale = af_core::etag::versioned(&created.fleet.id, i64::MAX);

    let refused = f
        .creations
        .confirm_delete(f.access(), &created.fleet.id, &stale)
        .await
        .expect_err("a stale tag is refused");

    assert_eq!(refused.code(), error_code::SETUP_STALE);
    assert_eq!(refused.etag(), Some(created.fleet.etag.as_str()));
    f.creations
        .confirm_delete(f.access(), &created.fleet.id, &created.fleet.etag)
        .await
        .expect("the current tag passes");
    assert_eq!(f.counts().await, (1, 1, 1), "the check deletes nothing");
    f.cleanup().await;
}
