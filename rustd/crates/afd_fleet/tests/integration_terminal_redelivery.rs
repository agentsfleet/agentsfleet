//! Dimension 7.4 — a redelivered event that already finished is acknowledged,
//! not executed.
//!
//! One logical event can sit on two stream entries, so an entry can come back
//! after its event ended. `pull.rs` answers that case before the gates, the
//! money, the secrets and the lease row, because each of those is an effect of
//! executing and the execution already happened: the tenant paid for it and its
//! answer is already owed or delivered.
//!
//! The acknowledgement is the point, not tidying up. An entry left pending is
//! offered again, so a terminal event that is only skipped returns on every
//! poll forever, each pass costing a selection, an insert attempt and a read.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints this without
//! datastores; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_datastore::ready::READY_PARTITIONS;
use afd_datastore::streams::FLEET_CONSUMER_GROUP;
use sqlx::Row as _;

use afd_fleet::lease::runner_consumer;
use afd_runner::sweep::rebuild::rebuild;
use afd_runner::sweep::reclaim::Reclaim;

use crate::queue;
use crate::report_seed::held;
use crate::support::Fixtures;

/// A stored config the runtime parser accepts.
///
/// `seeded` writes `{}`, which has no `x-agentsfleet` block, and the reclaim
/// path parses the config where a fresh poll does not -- so without this the
/// redelivery fails as `RuntimeBlockRequired` before it can be suppressed.
/// Same shape `integration_lease_installed.rs` installs.
const STORED_CONFIG: &str = r#"{"name":"probe","x-agentsfleet":{"triggers":[{"type":"api"}],
   "tools":[],"budget":{"daily_dollars":1.0}}}"#;

/// Far enough past the lease's expiry that the reclaim treats it as lapsed.
///
/// A parameter rather than a sleep: `reclaim_prior_active` takes `now`, so the
/// lapse is expressed by asking a later question, not by waiting out a TTL.
const LONG_AFTER_MS: i64 = 10 * 60 * 1000;

/// Dimension 7.4.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_completed_event_redelivery_does_not_execute() {
    let held = held().await;
    let before = leases_on(&held.fixtures, &held.fleet).await;

    // The event ENDS while its entry is still held. That is the whole setup:
    // one logical event can sit on two stream entries, so an entry can come
    // back after the event it names has already finished.
    held.fixtures
        .end_event(
            &held.fleet,
            &held.event_id,
            afd_core::event::status::PROCESSED,
        )
        .await;
    assert_eq!(
        pending_on(&held.fixtures, &held.fleet).await,
        1,
        "the finished event left its entry unacknowledged; without one held \
         there is no redelivery to suppress"
    );

    arrange_the_redelivery(&held).await;

    let lapsed = held.now.saturating_add_millis(LONG_AFTER_MS);
    poll_until_acknowledged(&held, lapsed).await;
    assert_eq!(
        pending_on(&held.fixtures, &held.fleet).await,
        0,
        "the entry was ACKNOWLEDGED. Left pending it is offered again on every \
         poll forever, which is the cost this path exists to end"
    );
    assert_eq!(
        leases_on(&held.fixtures, &held.fleet).await,
        before,
        "and no second lease row was written -- the stop lands before the \
         gates, the money, the secrets and the lease row, because every one of \
         those is an effect of executing"
    );

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// Puts the finished event back in front of a poll.
///
/// `XREADGROUP >` never re-offers a delivered entry, so the redelivery has to
/// come from the reclaim: a lease still `active` past its expiry is claimed
/// back and its entry handed to the next poll. The config install is not
/// scenery -- the reclaim path parses the fleet's config where a fresh poll
/// does not, so `seeded`'s `{}` refuses as `RuntimeBlockRequired` before the
/// suppression can be reached.
async fn arrange_the_redelivery(held: &crate::report_seed::Held) {
    queue::mark_ready(held.fixtures.queue(), &held.fleet).await;
    install_config(&held.fixtures, &held.fleet).await;
    expire_lease(&held.fixtures, held.issued.lease_id.as_str()).await;
    let reclaim = Reclaim::new(
        held.fixtures.database.clone(),
        held.fixtures.queue().clone(),
        runner_consumer(),
    );
    rebuild(&[&reclaim], 1)
        .await
        .expect("the reclaim pass must reach both datastores");
}

/// Polls one plane until this fleet's entry has been acknowledged.
///
/// Two things make a single `lease()` call the wrong unit here, and both were
/// diagnosed the hard way.
///
/// `select` advances a cursor and peeks ONE partition per poll
/// (`afd_fleet::lease::assign`), so reaching this fleet's partition takes up to
/// `READY_PARTITIONS` polls -- a measured probe found it on the sixteenth.
/// And the cursor belongs to the `Leases` the plane owns, so calling `plane()`
/// inside the loop would hand every poll a FRESH cursor, re-peeking one
/// partition forever and never arriving. One plane, hoisted, is what makes the
/// rotation a rotation.
///
/// It stops at the acknowledgement rather than running the rotation out, for
/// the reason `select_within_one_rotation` stops at its first `Some`: a poll
/// that keeps turning past its own fleet leases OTHER suites' ready fleets out
/// from under them, and this suite shares one readiness index. Stopping on the
/// effect is the only available signal, because a suppressed redelivery
/// answers `lease:null` exactly as an empty partition does.
///
/// Every answer must issue no lease: the fleet's only event has finished, so
/// the poll that finds it suppresses the redelivery and the rest find nothing.
async fn poll_until_acknowledged(
    held: &crate::report_seed::Held,
    now: afd_core::clock::UnixMillis,
) {
    let plane = held.fixtures.plane();
    for _poll in 0..READY_PARTITIONS {
        let answer = plane
            .lease(&held.runner, false, now)
            .await
            .expect("the poll must reach the datastore");
        // The reason rides the log rather than the JSON, so the claim is
        // proven by its two effects instead of by the answer's text.
        assert!(
            answer.contains("\"lease\":null"),
            "a suppressed redelivery issues no lease: {answer}"
        );
        if pending_on(&held.fixtures, &held.fleet).await == 0 {
            return;
        }
    }
}

/// Gives the seeded fleet a config the runtime parser can read.
async fn install_config(fixtures: &Fixtures, fleet: &str) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("UPDATE core.fleets SET config_json = $2::jsonb WHERE id = $1::uuid")
        .bind(fleet)
        .bind(STORED_CONFIG)
        .execute(&mut *connection)
        .await
        .expect("the config install must run");
}

/// Writes the lease's expiry into the past.
///
/// The sweeper compares `lease_expires_at` against the wall clock it carries,
/// not the `now` a poll is asked about, so a lapse has to be recorded rather
/// than passed in -- and `backdate_lease` moves `created_at`, which is a
/// different question.
async fn expire_lease(fixtures: &Fixtures, lease: &str) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("UPDATE fleet.runner_leases SET lease_expires_at = 1 WHERE id = $1::uuid")
        .bind(lease)
        .execute(&mut *connection)
        .await
        .expect("the expiry must run");
}

/// Entries the lease group has handed out and not had acknowledged.
async fn pending_on(fixtures: &Fixtures, fleet: &str) -> usize {
    let key = afd_datastore::fleet_stream_key(fleet);
    let mut cmd = redis::cmd("XPENDING");
    cmd.arg(&key).arg(FLEET_CONSUMER_GROUP);
    let reply: redis::streams::StreamPendingReply = fixtures
        .queue()
        .command("XPENDING", &key, &cmd)
        .await
        .expect("XPENDING must answer on the lease group");
    reply.count()
}

/// How many lease rows the fleet holds — the executor's footprint.
///
/// A lease row is the first durable effect of deciding to execute, so a count
/// that does not move is the claim "the executor was not invoked" in the one
/// form the datastore can answer.
async fn leases_on(fixtures: &Fixtures, fleet: &str) -> i64 {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("SELECT count(*) FROM fleet.runner_leases WHERE fleet_id = $1::uuid")
        .bind(fleet)
        .fetch_one(&mut *connection)
        .await
        .expect("the count must run")
        .try_get(0)
        .expect("count answers a bigint")
}
