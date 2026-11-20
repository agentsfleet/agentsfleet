//! Save on a setup turn: a message that asks to save is answered by a setup
//! run like any other, and its report saves the head candidate the person
//! reviewed, with no follow-up request from the browser. A cron trigger the
//! model proposed on an earlier turn is the saved fleet's schedule.

use af_core::id::Uuid7;

use super::{Case, definition, processed, proposal};

/// The schedule the model proposes.
const SCHEDULE: &str = "0 9 * * *";

/// The saved fleet's schedules: each cron expression, zone and message.
async fn schedules(case: &Case, fleet: &Uuid7) -> Vec<(String, String, String)> {
    sqlx::query_as(
        "SELECT cron_expression, timezone, message FROM fleet.schedules \
         WHERE fleet_id = $1 ORDER BY created_at",
    )
    .bind(fleet)
    .fetch_all(&mut *case.connection().await)
    .await
    .expect("the fleet's schedules")
}

/// The saved fleet's head revision: its number and the candidate it saved.
async fn head_revision(case: &Case, fleet: &Uuid7) -> (i64, Option<Uuid7>) {
    sqlx::query_as(
        "SELECT r.revision, r.source_candidate_id FROM fleet.fleets f \
         JOIN fleet.revisions r ON r.id = f.head_revision_id WHERE f.id = $1",
    )
    .bind(fleet)
    .fetch_one(&mut *case.connection().await)
    .await
    .expect("the saved fleet's head revision")
}

// Value: protects=a person's Save in chat saves the setup they reviewed, with no
// browser follow-up; fails_when=the report drops save_requested, or applies the save
// turn's own proposal instead of the head; why_new=the chat-save suites left with the
// daemon's model path; seam=the production send, draft assignment and report_draft
// over live Postgres.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_save_requested_setup_turn_saves_the_head_candidate() {
    let case = Case::queued().await;
    let first = case.take_on_host().await;
    case.report(&processed(&first, None))
        .await
        .expect("the first turn settles");
    assert_eq!(
        case.saved_fleet().await,
        None,
        "a turn that did not ask to save saves nothing"
    );

    case.send_saving().await;
    let saving = case.take_on_host().await;
    // A save turn proposes nothing: a proposal it carries is not applied.
    let text = definition(SCHEDULE);
    case.report(&processed(&saving, Some(proposal(&saving, &text))))
        .await
        .expect("the save turn settles");

    let fleet = case
        .saved_fleet()
        .await
        .expect("the save turn's report saved a fleet");
    let (count, head) = case.candidates().await;
    assert_eq!(count, 1, "the save turn wrote no candidate");
    assert_eq!(
        head_revision(&case, &fleet).await,
        (1, head.clone()),
        "revision 1 is the head the person reviewed"
    );
    assert_eq!(
        schedules(&case, &fleet).await,
        Vec::new(),
        "the saved head declares no trigger"
    );
    let turn = case.turn(&saving.turn_id()).await;
    assert_eq!(turn.status, "completed", "{turn:?}");
    assert_eq!(turn.candidate, head, "the turn names the saved head");
    let refused: Option<String> =
        sqlx::query_scalar("SELECT save_error_code FROM thread.turns WHERE id = $1")
            .bind(saving.turn_id())
            .fetch_one(&mut *case.connection().await)
            .await
            .expect("the save turn");
    assert_eq!(refused, None, "the save was not refused");
    case.cleanup().await;
}

// Value: protects=a cron the model proposes in chat runs once the setup is saved;
// fails_when=the save writes no schedule, or a schedule other than the proposal's;
// why_new=the proposed-schedule suite left with the daemon's model path; seam=the
// production send, draft assignment and report_draft over live Postgres.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_proposed_cron_becomes_the_saved_fleets_schedule() {
    let case = Case::queued().await;
    let proposing = case.take_on_host().await;
    let text = definition(SCHEDULE);
    case.report(&processed(&proposing, Some(proposal(&proposing, &text))))
        .await
        .expect("the proposing turn settles");
    assert_eq!(case.candidates().await.0, 2, "the proposal is the head");
    assert_eq!(case.saved_fleet().await, None, "nothing saved yet");

    case.send_saving().await;
    let saving = case.take_on_host().await;
    case.report(&processed(&saving, None))
        .await
        .expect("the save turn settles");

    let fleet = case
        .saved_fleet()
        .await
        .expect("the save turn's report saved a fleet");
    assert_eq!(
        schedules(&case, &fleet).await,
        vec![(
            SCHEDULE.to_owned(),
            "UTC".to_owned(),
            "Daily check".to_owned()
        )],
        "one cron trigger, one schedule"
    );
    case.cleanup().await;
}
