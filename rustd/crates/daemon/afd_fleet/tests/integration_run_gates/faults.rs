//! Failed reads and parks, repeated refusals, and claims another runner won.

mod parking;

use super::seed::seed_provider_resolution;
use super::*;

use af_core::test_util::trace::Capture;

use crate::rows::{LAPSE, Row};

/// The stored status of an event nobody ended.
const STATUS_RECEIVED_ROW: &str = "received";

/// A stored document the runtime parser cannot read: no triggers, no runtime.
const UNREADABLE_CONFIG: &str = "{}";

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn should_refuse_an_unreadable_fleet_once_however_often_it_is_assigned() {
    // The first pass ends the event; the second finds the row already ended
    // and must neither reopen it nor answer anything but no work.
    let fixtures = Fixtures::create().await;
    let seeded = ready(&fixtures).await;
    fixtures
        .corrupt_native_config(&seeded.fleet, UNREADABLE_CONFIG)
        .await;
    let claimed = claim(&fixtures, &seeded).await;

    let first = drive(&fixtures, &seeded, claimed.clone()).await;
    let ended = terminal_of(&fixtures, &seeded.fleet, &seeded.turn_id).await;
    let second = drive(&fixtures, &seeded, claimed).await;

    let expected = Some((
        STATUS_GATE_BLOCKED.to_owned(),
        af_core::turn::label::CONFIG_UNREADABLE.to_owned(),
    ));
    assert!(first.is_none() && second.is_none());
    assert_eq!(ended, expected, "the first pass ends the event");
    assert_eq!(
        terminal_of(&fixtures, &seeded.fleet, &seeded.turn_id).await,
        expected,
        "the second pass leaves the ended row as it was"
    );

    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_unreadable_redelivery_preserves_an_existing_terminal_ending() {
    let fixtures = Fixtures::create().await;
    let seeded = ready(&fixtures).await;
    let claimed = claim(&fixtures, &seeded).await;
    fixtures
        .end_event(
            &seeded.fleet,
            &seeded.turn_id,
            af_core::turn::status::PROCESSED,
        )
        .await;
    let before = afd_turns::view::fleet_counters(&fixtures.database, &seeded.fleet)
        .await
        .expect("counters");
    fixtures
        .corrupt_native_config(&seeded.fleet, UNREADABLE_CONFIG)
        .await;
    // The holder stops renewing, and the finished event is claimed again under
    // its now unreadable setup: a terminal redelivery.
    fixtures
        .age(Row::Slot(&claimed.fleet_id), "run_expires_at", LAPSE)
        .await;
    let again = claim(&fixtures, &seeded).await;
    assert_eq!(
        again.opened.received.delivery,
        afd_fleet::run::Delivery::Terminal
    );
    let answer = drive(&fixtures, &seeded, again).await;
    assert!(answer.is_none());
    assert_eq!(
        terminal_of(&fixtures, &seeded.fleet, &seeded.turn_id).await,
        Some((af_core::turn::status::PROCESSED.to_owned(), String::new()))
    );
    assert!(fixtures.admission_settled(&seeded.turn_id).await);
    assert_eq!(
        afd_turns::view::fleet_counters(&fixtures.database, &seeded.fleet)
            .await
            .expect("unchanged counters"),
        before
    );
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM thread.turns WHERE id=$1 AND opened_at IS NOT NULL",
    )
    .bind(&seeded.turn_id)
    .fetch_one(&mut *fixtures.database.acquire().await.expect("observer"))
    .await
    .expect("one narrative");
    assert_eq!(rows, 1);
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn should_answer_no_work_when_an_unreadable_fleets_refusal_cannot_be_recorded() {
    let fixtures = Fixtures::create_isolated().await;
    let seeded = ready(&fixtures).await;
    fixtures
        .corrupt_native_config(&seeded.fleet, UNREADABLE_CONFIG)
        .await;
    let claimed = claim(&fixtures, &seeded).await;
    let owner = fixtures.open(afd_db::config::DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("owner");
    // The claim opened the event; the gate ending that would close it is refused.
    // Only a status change: the turn's other writes, settling it among them, are
    // not the narrative.
    sqlx::raw_sql("CREATE FUNCTION fleet.refuse_narrative() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected narrative failure'; END $$; CREATE TRIGGER refuse_narrative BEFORE UPDATE OF status ON thread.turns FOR EACH ROW WHEN (OLD.status IS DISTINCT FROM NEW.status) EXECUTE FUNCTION fleet.refuse_narrative()")
        .execute(&mut *connection).await.expect("inject narrative failure");
    drop(connection);
    let log = Capture::install();
    let answer = drive(&fixtures, &seeded, claimed).await;
    let line = log.only("config_refusal_unrecorded").fields;
    drop(log);

    assert!(
        answer.is_none(),
        "an unrecordable refusal issued a run: {answer:?}"
    );
    assert!(line.get("error_code").is_some_and(|code| !code.is_empty()));
    assert_eq!(
        line.get("agentsfleet_turn_id").map(String::as_str),
        Some(seeded.turn_id.as_str())
    );
    assert_eq!(
        terminal_of(&fixtures, &seeded.fleet, &seeded.turn_id).await,
        Some((STATUS_RECEIVED_ROW.to_owned(), String::new())),
        "the row the claim opened stays open when its refusal cannot be recorded"
    );

    owner.close().await;
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn should_write_no_run_when_another_runner_won_the_claim_mid_pass() {
    // Every gate admits the run, and by the time the run row is written a
    // second runner holds the slot. The fenced insert writes nothing, and the
    // pass answers no work rather than a run two runners would execute.
    let fixtures = Fixtures::create().await;
    let (fleet, workspace, tenant, [runner, rival]) = seeded_parts::<2>(&fixtures).await;
    let seeded = ready_in(&fixtures, &workspace, runner, fleet, tenant).await;
    set_config(&fixtures, &seeded.fleet, READ_BOUND_CONFIG).await;
    let _default = seed_provider_resolution(&fixtures, &seeded.fleet).await;
    let claimed = claim(&fixtures, &seeded).await;
    let slot = Row::Slot(&claimed.fleet_id);
    fixtures.age(slot, "run_expires_at", LAPSE).await;
    let won = fixtures
        .runs()
        .claim(&claimed.fleet_id, &rival, af_core::timing::RUN_TTL)
        .await
        .expect("the claim statement answers");
    assert!(won.is_some(), "the rival takes the lapsed slot");

    let log = Capture::install();
    let answer = drive(&fixtures, &seeded, claimed).await;
    let lost = log.only("run_claim_lost").fields;
    drop(log);

    assert!(answer.is_none(), "a lost claim issued a run: {answer:?}");
    assert_eq!(
        lost.get("runner_id").map(String::as_str),
        Some(seeded.runner.as_str())
    );
    assert_eq!(
        lost.get("agentsfleet_turn_id").map(String::as_str),
        Some(seeded.turn_id.as_str())
    );

    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn should_write_no_run_when_the_pass_outlived_its_claim() {
    // Every gate admits the run and no rival takes the slot, but the claim's
    // own deadline passed while the gates ran. The issue refuses a claim past
    // its lapse rather than write a run another runner may soon hold.
    let fixtures = Fixtures::create().await;
    let seeded = ready(&fixtures).await;
    set_config(&fixtures, &seeded.fleet, READ_BOUND_CONFIG).await;
    let _default = seed_provider_resolution(&fixtures, &seeded.fleet).await;
    let mut claimed = claim(&fixtures, &seeded).await;
    claimed.held_until = jiff::Timestamp::UNIX_EPOCH;

    let log = Capture::install();
    let answer = drive(&fixtures, &seeded, claimed).await;
    let lost = log.only("run_claim_lost");
    drop(log);

    assert_eq!(
        (
            answer.is_none(),
            lost.field("runner_id"),
            lost.field("agentsfleet_turn_id"),
        ),
        (
            true,
            Some(seeded.runner.as_str()),
            Some(seeded.turn_id.as_str())
        ),
        "a lapsed claim issued a run: {answer:?}"
    );
    fixtures.cleanup().await;
}

/// A [`Ready`] whose `chat` event is enqueued under `workspace`, which the
/// caller picks: the fleet's own, or one no row carries.
async fn ready_in(
    fixtures: &Fixtures,
    workspace: &Uuid7,
    runner: Uuid7,
    fleet: Uuid7,
    tenant: Uuid7,
) -> Ready {
    let turn_id = crate::queue::enqueue(
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
        turn_id,
        tenant,
    }
}
