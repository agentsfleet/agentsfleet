//! A delete holds its fleet from the tag check to the purge: a stale tag is
//! refused holding nothing, a change made meanwhile waits for it, and a purge
//! that finds a schedule still registered changes nothing.

use std::time::Duration;

use af_core::error_code;
use af_core::id::Uuid7;
use afd_api_wire::fleet::PatchFleetRequest;

use super::fixture::{Fixture, patch, valid};
use super::mint;

/// How often the window case looks for the change waiting on the held row.
const LOCK_POLL: Duration = Duration::from_millis(10);

/// How long a change waits on a held row before giving up: the API role's
/// `lock_timeout` (`afd_db::login::LOCK_TIMEOUT`, 2000ms).
const LOCK_WAIT: Duration = Duration::from_millis(2000);

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

/// A fleet the owner created on the fixture's model.
async fn created(f: &Fixture) -> afd_setup::types::SavedFleet {
    f.creations
        .create(
            f.access(),
            &valid(f.request(&mint())),
            super::fixture::now(),
        )
        .await
        .expect("the owner creates a fleet")
        .fleet
}

/// Waits until another session is blocked on the backend `holder`, which is
/// the change queued behind the held delete, reading through `watcher`, a
/// connection of its own so the wait never queues on the fixture's pool.
///
/// Bounded by the API role's lock timeout: past it the change has given up
/// waiting, so a watcher that has not seen it fails rather than spinning.
async fn blocked_on(watcher: &mut sqlx::PgConnection, holder: i32) {
    let deadline = tokio::time::Instant::now() + LOCK_WAIT;
    loop {
        let waiting: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)))",
        )
        .bind(holder)
        .fetch_one(&mut *watcher)
        .await
        .expect("the session list reads");
        if waiting {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "no change queued behind {holder} within the lock timeout"
        );
        tokio::time::sleep(LOCK_POLL).await;
    }
}

// Value: protects=the cascade never takes a schedule the scheduler may still
// hold; fails_when=the purge deletes a fleet that holds a schedule;
// seam=`Creations::delete` over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_delete_of_a_fleet_holding_a_schedule_is_refused_and_changes_nothing() {
    let f = Fixture::create().await;
    let fleet = created(&f).await;
    schedule_on(&f, &fleet.id).await;

    let held = f
        .creations
        .hold_delete(f.access(), &fleet.id, &fleet.etag)
        .await
        .expect("the current tag holds the fleet");
    let refused = f
        .creations
        .delete(held)
        .await
        .expect_err("a fleet still holding a schedule must not go");

    assert_eq!(refused.code(), error_code::SCHEDULE_STILL_REGISTERED);
    assert_eq!(refused.current_state(), Some("deleting"));
    assert_eq!(
        refused.detail(),
        "Fleet schedules still registered; repeat the delete"
    );
    assert_eq!(
        f.counts().await,
        (1, 1, 1),
        "the fleet, thread and turn stay"
    );
    f.cleanup().await;
}

// Value: protects=a stale tag is refused before a delete removes any schedule
// upstream; fails_when=the hold passes a tag that is not current;
// seam=`Creations::hold_delete` over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_held_delete_refuses_a_stale_tag_and_holds_the_current_one() {
    let f = Fixture::create().await;
    let fleet = created(&f).await;
    let stale = af_core::etag::versioned(&fleet.id, i64::MAX);

    let refused = f
        .creations
        .hold_delete(f.access(), &fleet.id, &stale)
        .await
        .expect_err("a stale tag is refused");

    assert_eq!(refused.code(), error_code::SETUP_STALE);
    assert_eq!(refused.etag(), Some(fleet.etag.as_str()));
    let held = f
        .creations
        .hold_delete(f.access(), &fleet.id, &fleet.etag)
        .await
        .expect("the current tag holds the fleet");
    drop(held);
    assert_eq!(
        f.counts().await,
        (1, 1, 1),
        "a hold dropped before its purge deletes nothing"
    );
    f.cleanup().await;
}

// Value: protects=a change racing a delete cannot stale the delete's tag after
// its schedules are gone; fails_when=the hold takes no lock a change waits on,
// so the patch commits and the purge's delete runs under a bumped tag;
// seam=`Creations::hold_delete` then `Creations::delete` beside
// `Creations::patch` over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_change_during_a_held_delete_waits_and_the_delete_keeps_its_tag() {
    let f = Fixture::create().await;
    let fleet = created(&f).await;
    let mut held = f
        .creations
        .hold_delete(f.access(), &fleet.id, &fleet.etag)
        .await
        .expect("the current tag holds the fleet");
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(held.connection())
        .await
        .expect("the held transaction names its backend");
    let rename = PatchFleetRequest {
        name: Some("support-triage".to_owned()),
        ..patch()
    };
    let rename = valid(rename);
    let owner = f.owner_db().await;
    let mut watcher = owner.acquire().await.expect("the watcher connects");

    let (patched, deleted) = tokio::join!(
        f.creations
            .patch(f.access(), &fleet.id, Some(&fleet.etag), &rename),
        async {
            blocked_on(&mut watcher, holder).await;
            f.creations.delete(held).await
        },
    );

    deleted.expect("the delete goes through under the tag it checked");
    let refused = patched.expect_err("the change waited, then found the fleet gone");
    assert_eq!(refused.code(), error_code::AGENTSFLEET_NOT_FOUND);
    assert_eq!(f.counts().await, (0, 0, 0), "the fleet went whole");
    f.cleanup().await;
}
