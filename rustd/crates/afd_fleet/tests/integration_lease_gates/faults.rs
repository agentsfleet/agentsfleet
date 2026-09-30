//! The pass's failure and repeat arms, each driven by a real injected failure:
//! a queue that will not acknowledge, a payer read that will not answer, and
//! an unreadable fleet offered a second time.

use super::*;

use afd_billing::Accounts;
use afd_core::test_util::trace::Capture;
use afd_crypto::entropy::Entropy;
use afd_dragonfly::FleetStreams;
use afd_fleet::lease::Plane;
use afd_fleet::lease::admit::Refusal;

use crate::lease_reads::COLUMN_LEASED_UNTIL;

/// The stored status of an event nobody ended.
const STATUS_RECEIVED_ROW: &str = "received";

/// A stored document the runtime parser cannot read: no triggers, no runtime.
const UNREADABLE_CONFIG: &str = "{}";

/// A consumer that only inspects what is still owed.
const INSPECTOR: &str = "agentsfleetd-fault-inspector";

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn should_answer_no_work_and_keep_the_entry_when_a_finished_event_cannot_be_acknowledged() {
    // The event already ran: its row is terminal. The acknowledgement that
    // would end its redelivery fails, and the pass must still answer no work
    // — the entry stays owed and the next poll acknowledges it instead.
    let fixtures = Fixtures::create_with_queue().await;
    let seeded = ready(&fixtures).await;
    set_config(&fixtures, &seeded.fleet, BUDGETED_CONFIG).await;
    let claimed = claim(&fixtures, &seeded).await;
    let leases = fixtures.leases();
    let now = UnixMillis::from_millis(ENROLLED_AT);
    leases
        .record_received(&claimed, now)
        .await
        .expect("the row opens");
    let label = afd_core::event::label::EVENT_TYPE_UNSUPPORTED;
    leases
        .block(
            &claimed.fleet_id,
            &claimed.event_id,
            Refusal::labelled(label),
            now,
        )
        .await
        .expect("the row ends");

    let log = Capture::install();
    let answer = fixtures
        .plane_with_dead_queue()
        .lease_claimed(claimed.clone(), &seeded.runner, now)
        .await
        .expect("a lost acknowledgement is not the runner's fault");
    let failed = log.only("terminal_redelivery_ack_failed").fields;
    let suppressed = log.only("terminal_redelivery_suppressed").fields;
    drop(log);

    assert!(
        answer.contains(NO_LEASE),
        "a finished event issued a lease: {answer}"
    );
    assert!(
        failed
            .get("error_code")
            .is_some_and(|code| !code.is_empty())
    );
    assert_eq!(failed.get("agentsfleet_event_id"), Some(&seeded.event_id));
    assert_eq!(
        suppressed.get("agentsfleet_event_id"),
        Some(&seeded.event_id)
    );
    let owed = FleetStreams::new(fixtures.queue().clone())
        .take_over_oldest(&seeded.fleet, INSPECTOR)
        .await
        .expect("the stream answers")
        .expect("the unacknowledged entry is still owed");
    assert_eq!(owed.receipt, claimed.receipt);

    fixtures.cleanup().await;
}

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
    let plane = Plane {
        accounts: Accounts::new(afd_db::test_util::unreachable_db(), Entropy::new()),
        ..fixtures.plane()
    };

    let now = UnixMillis::from_millis(ENROLLED_AT);
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
    assert_eq!(
        fixtures
            .affinity_column(&seeded.fleet, COLUMN_LEASED_UNTIL)
            .await,
        Some(ENROLLED_AT.to_string()),
        "the retry freed the claim"
    );

    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn should_refuse_an_unreadable_fleet_once_however_often_it_is_offered() {
    // The first pass ends the event; the second finds the row already ended
    // and must neither reopen it nor answer anything but no work.
    let fixtures = Fixtures::create_with_queue().await;
    let seeded = ready(&fixtures).await;
    set_config(&fixtures, &seeded.fleet, UNREADABLE_CONFIG).await;
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
async fn should_answer_no_work_when_an_unreadable_fleets_refusal_cannot_be_recorded() {
    // The fleet's document will not parse, so the pass ends its event — but
    // the entry names a workspace no row carries, and the event row cannot be
    // written. The runner is still answered no work rather than an error: the
    // fault is another fleet's, and the entry stays owed for a later poll.
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, _workspace, tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    set_config(&fixtures, &fleet, UNREADABLE_CONFIG).await;
    let nowhere = crate::seed::unique_ids().1;
    let event_id = crate::queue::enqueue(
        fixtures.queue(),
        &fleet,
        &nowhere,
        ACTOR,
        EVENT_TYPE_CHAT,
        REQUEST_JSON,
        ENROLLED_AT,
    )
    .await;
    let seeded = Ready {
        runner,
        fleet,
        event_id,
        tenant,
    };
    let claimed = claim(&fixtures, &seeded).await;

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

    fixtures.cleanup().await;
}
