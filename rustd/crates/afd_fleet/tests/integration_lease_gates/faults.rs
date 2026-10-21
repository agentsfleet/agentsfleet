//! The pass's failure and repeat arms, each driven by a real injected failure:
//! a queue that will not acknowledge, a payer read that will not answer, an
//! unreadable fleet offered a second time, and a claim another runner won.

use super::seed::seed_provider_resolution;
use super::*;

use afd_billing::Accounts;
use afd_core::test_util::trace::Capture;
use afd_fleet::lease::Plane;

use crate::rows::{LAPSE, Row};

/// The stored status of an event nobody ended.
const STATUS_RECEIVED_ROW: &str = "received";

/// A stored document the runtime parser cannot read: no triggers, no runtime.
const UNREADABLE_CONFIG: &str = "{}";

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn should_retry_and_free_the_claim_when_the_payer_cannot_be_read() {
    // A payer read that fails is neither an owner-less workspace to refuse nor
    // a tenant to bill: the pass answers no work, the event stays runnable,
    // and the claim is freed so the next poll can read the payer again.
    let fixtures = Fixtures::create_with_queue().await;
    let seeded = ready(&fixtures).await;
    set_config(&fixtures, &seeded.fleet, BUDGETED_CONFIG).await;
    let claimed = claim(&fixtures, &seeded).await;
    let plane = plane_with_unreadable_payer(&fixtures);

    let now = fixtures.enrolled_at();
    let log = Capture::install();
    let answer = plane
        .lease_claimed(claimed.clone(), &seeded.runner, now)
        .await
        .expect("an unread payer is a retry, not a fault");
    let line = log.only("lease_tenant_lookup_failed").fields;
    drop(log);
    // The next poll meets the same delivery on its open row: it retries again
    // and reopens nothing.
    let again = plane
        .lease_claimed(claimed, &seeded.runner, now)
        .await
        .expect("the retry repeats");

    assert!(
        answer.contains(NO_LEASE),
        "an unread payer issued a lease: {answer}"
    );
    assert!(
        again.contains(NO_LEASE),
        "the repeat issued a lease: {again}"
    );
    assert_eq!(
        line.get("error_code").map(String::as_str),
        Some(afd_core::error_code::INTERNAL_DB_QUERY.as_str())
    );
    assert_eq!(
        terminal_of(&fixtures, &seeded.fleet, &seeded.event_id).await,
        Some((STATUS_RECEIVED_ROW.to_owned(), String::new())),
        "a retry ends nothing"
    );
    assert_claim_freed(&fixtures, &seeded.fleet).await;

    fixtures.cleanup().await;
}

/// The fixture plane with a payer ledger whose database never answers.
fn plane_with_unreadable_payer(fixtures: &Fixtures) -> Plane {
    Plane {
        accounts: Accounts::new(afd_db::test_util::unreachable_db()),
        ..fixtures.plane()
    }
}

/// Asserts the fleet's claim was handed back, so the next poll may take it.
async fn assert_claim_freed(fixtures: &Fixtures, fleet: &str) {
    assert!(
        fixtures.slot_is_free(fleet).await,
        "the retry freed the claim"
    );
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn should_refuse_an_unreadable_fleet_once_however_often_it_is_offered() {
    // The first pass ends the event; the second finds the row already ended
    // and must neither reopen it nor answer anything but no work.
    let fixtures = Fixtures::create_with_queue().await;
    let seeded = ready(&fixtures).await;
    fixtures
        .corrupt_native_config(&seeded.fleet, UNREADABLE_CONFIG)
        .await;
    let claimed = claim(&fixtures, &seeded).await;

    let first = drive(&fixtures, &seeded, claimed.clone()).await;
    let ended = terminal_of(&fixtures, &seeded.fleet, &seeded.event_id).await;
    let second = drive(&fixtures, &seeded, claimed).await;

    let expected = Some((
        STATUS_GATE_BLOCKED.to_owned(),
        afd_core::event::label::CONFIG_UNREADABLE.to_owned(),
    ));
    assert!(first.contains(NO_LEASE) && second.contains(NO_LEASE));
    assert_eq!(ended, expected, "the first pass ends the event");
    assert_eq!(
        terminal_of(&fixtures, &seeded.fleet, &seeded.event_id).await,
        expected,
        "the second pass leaves the ended row as it was"
    );

    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_unreadable_redelivery_preserves_an_existing_terminal_ending() {
    let fixtures = Fixtures::create_with_queue().await;
    let seeded = ready(&fixtures).await;
    let claimed = claim(&fixtures, &seeded).await;
    fixtures
        .leases()
        .record_received(&claimed)
        .await
        .expect("first delivery");
    fixtures
        .end_event(
            &seeded.fleet,
            &seeded.event_id,
            afd_core::event::status::PROCESSED,
        )
        .await;
    let before = afd_events::fleet_counters(&fixtures.database, &seeded.fleet)
        .await
        .expect("counters");
    fixtures
        .corrupt_native_config(&seeded.fleet, UNREADABLE_CONFIG)
        .await;
    let answer = drive(&fixtures, &seeded, claimed).await;
    assert!(answer.contains(NO_LEASE));
    assert_eq!(
        terminal_of(&fixtures, &seeded.fleet, &seeded.event_id).await,
        Some((afd_core::event::status::PROCESSED.to_owned(), String::new()))
    );
    assert!(fixtures.admission_settled(&seeded.event_id).await);
    assert_eq!(
        afd_events::fleet_counters(&fixtures.database, &seeded.fleet)
            .await
            .expect("unchanged counters"),
        before
    );
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM fleet.events WHERE event_id=$1")
        .bind(&seeded.event_id)
        .fetch_one(&mut *fixtures.database.acquire().await.expect("observer"))
        .await
        .expect("one narrative");
    assert_eq!(rows, 1);
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn should_answer_no_work_when_an_unreadable_fleets_refusal_cannot_be_recorded() {
    let fixtures = Fixtures::create_isolated_with_queue().await;
    let seeded = ready(&fixtures).await;
    fixtures
        .corrupt_native_config(&seeded.fleet, UNREADABLE_CONFIG)
        .await;
    let claimed = claim(&fixtures, &seeded).await;
    let owner = fixtures.open(afd_db::config::DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("owner");
    sqlx::raw_sql("CREATE FUNCTION fleet.refuse_narrative() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected narrative failure'; END $$; CREATE TRIGGER refuse_narrative BEFORE INSERT ON fleet.events FOR EACH ROW EXECUTE FUNCTION fleet.refuse_narrative()")
        .execute(&mut *connection).await.expect("inject narrative failure");
    drop(connection);
    let log = Capture::install();
    let answer = drive(&fixtures, &seeded, claimed).await;
    let line = log.only("config_refusal_unrecorded").fields;
    drop(log);

    assert!(
        answer.contains(NO_LEASE),
        "an unrecordable refusal issued a lease: {answer}"
    );
    assert!(line.get("error_code").is_some_and(|code| !code.is_empty()));
    assert_eq!(line.get("agentsfleet_event_id"), Some(&seeded.event_id));
    assert_eq!(
        terminal_of(&fixtures, &seeded.fleet, &seeded.event_id).await,
        None,
        "no row was written for an event whose refusal could not be recorded"
    );

    owner.close().await;
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn should_write_no_lease_when_another_runner_won_the_claim_mid_pass() {
    // Every gate admits the run, and by the time the lease row is written a
    // second runner holds the slot. The fenced insert writes nothing, and the
    // pass answers no work rather than a lease two runners would execute.
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, tenant, [runner, rival]) = seeded_parts::<2>(&fixtures).await;
    let seeded = ready_in(&fixtures, &workspace, runner, fleet, tenant).await;
    set_config(&fixtures, &seeded.fleet, READ_BOUND_CONFIG).await;
    let _default = seed_provider_resolution(&fixtures, &seeded.fleet).await;
    let claimed = claim(&fixtures, &seeded).await;
    let slot = Row::Slot(claimed.fleet_id.as_str());
    fixtures.age(slot, "lease_expires_at", LAPSE).await;
    let won = fixtures
        .leases()
        .claim(&claimed.fleet_id, &rival, afd_core::timing::LEASE_TTL)
        .await
        .expect("the claim statement answers");
    assert!(won.is_some(), "the rival takes the lapsed slot");

    let log = Capture::install();
    let answer = drive(&fixtures, &seeded, claimed).await;
    let lost = log.only("lease_claim_lost").fields;
    drop(log);

    assert!(
        answer.contains(NO_LEASE),
        "a lost claim issued a lease: {answer}"
    );
    assert_eq!(
        lost.get("runner_id"),
        Some(&seeded.runner.as_str().to_owned())
    );
    assert_eq!(lost.get("agentsfleet_event_id"), Some(&seeded.event_id));

    fixtures.cleanup().await;
}

/// A [`Ready`] whose `chat` event is enqueued under `workspace`, which the
/// caller picks: the fleet's own, or one no row carries.
async fn ready_in(
    fixtures: &Fixtures,
    workspace: &str,
    runner: Uuid7,
    fleet: String,
    tenant: String,
) -> Ready {
    let event_id = crate::queue::enqueue(
        &fixtures.database,
        &fleet,
        workspace,
        ACTOR,
        EVENT_TYPE_CHAT,
        REQUEST_JSON,
        fixtures.enrolled_at(),
    )
    .await;
    Ready {
        runner,
        fleet,
        event_id,
        tenant,
    }
}
