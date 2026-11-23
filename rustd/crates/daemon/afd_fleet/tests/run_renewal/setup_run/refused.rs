//! The report's own check is a safety net: a proposal it refuses writes no
//! candidate, the turn is still answered, and the refusal is kept as the
//! turn's failure detail and logged once under its registry code.

use af_core::{error_code, test_util::trace::Capture};

use super::{ANSWER, Case, definition, processed, proposal};

/// The event a refused proposal logs.
const REFUSED: &str = "setup_proposal_refused";

/// Minute 61 is outside its field: a Definition the daemon's parser refuses.
const MINUTE_61: &str = "61 * * * *";

// Value: protects=a proposal the daemon refuses never becomes a candidate, and the
// person still gets the answer; fails_when=the report check is dropped, or a refusal
// fails or drops the answer; why_new=the refusal now keeps its detail and its event;
// seam=the production draft assignment and report_draft over live Postgres.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_report_whose_proposal_fails_the_daemon_check_writes_no_candidate() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;
    let (base, _revision) = taken.base();
    let base = af_core::id::Uuid7::parse(base).expect("the base is a UUIDv7");
    let text = definition(MINUTE_61);

    let captured = Capture::install();
    case.report(&processed(&taken, Some(proposal(&taken, &text))))
        .await
        .expect("a refused proposal still settles the run");
    let refused = captured.only(REFUSED);
    drop(captured);

    assert_eq!(
        refused.field("error_code"),
        Some(error_code::AGENTSFLEET_INVALID_CONFIG.as_str())
    );
    let turn = case.turn(&taken.turn_id()).await;
    assert_eq!(turn.status, "completed", "{turn:?}");
    assert_eq!(turn.answer.as_deref(), Some(ANSWER), "the answer stands");
    assert_eq!(
        turn.candidate.as_ref(),
        Some(&base),
        "the head is unchanged"
    );
    assert_eq!(turn.failure_reason, None, "an answered turn did not fail");
    assert!(
        turn.failure_detail
            .as_deref()
            .is_some_and(|detail| detail.contains("`schedule`")),
        "the detail names the field the model got wrong: {turn:?}"
    );
    assert_eq!(
        case.candidates().await,
        (1, Some(base)),
        "no candidate written"
    );
    assert_eq!(
        case.runs().await,
        vec![(1, afd_state::sql::RUN_STATUS_REPORTED.to_owned())],
        "the run settled"
    );
    case.cleanup().await;
}

// Value: protects=an operator can find a refused proposal by code, draft and run without
// reading private text; fails_when=the event loses a field, or carries the Definition;
// why_new=the event was `setup_report_not_applied` with the draft only;
// seam=report_draft's refusal over live Postgres, read through the tracing capture.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_refused_report_proposal_logs_its_error_code() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;
    let text = definition(MINUTE_61);

    let captured = Capture::install();
    case.report(&processed(&taken, Some(proposal(&taken, &text))))
        .await
        .expect("a refused proposal still settles the run");
    let refused = captured.only(REFUSED);
    drop(captured);

    assert_eq!(refused.level, tracing::Level::WARN);
    assert_eq!(
        refused.field("error_code"),
        Some(error_code::AGENTSFLEET_INVALID_CONFIG.as_str())
    );
    assert_eq!(refused.field("draft_id"), Some(case.draft.id.as_str()));
    assert_eq!(refused.field("run_id"), Some(taken.run.as_str()));
    assert!(
        refused
            .fields
            .values()
            .all(|value| !value.contains(MINUTE_61) && !value.contains("Daily check")),
        "no definition text reaches the log: {refused:?}"
    );
    case.cleanup().await;
}

// Value: protects=a proposal on a head that moved is refused as stale, not applied;
// fails_when=the base check is dropped from the report; why_new=the stale refusal now
// logs UZ-SETUP-003 and keeps its detail; seam=report_draft over live Postgres.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_report_on_a_moved_base_writes_no_candidate_and_logs_stale() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;
    let (base, revision) = taken.base();
    let base = af_core::id::Uuid7::parse(base).expect("the base is a UUIDv7");
    let text = definition("0 9 * * *");
    let mut moved = proposal(&taken, &text);
    moved.base_revision = revision + 1;

    let captured = Capture::install();
    case.report(&processed(&taken, Some(moved)))
        .await
        .expect("a stale proposal still settles the run");
    let refused = captured.only(REFUSED);
    drop(captured);

    assert_eq!(
        refused.field("error_code"),
        Some(error_code::SETUP_STALE.as_str())
    );
    let turn = case.turn(&taken.turn_id()).await;
    assert_eq!(turn.status, "completed", "{turn:?}");
    assert_eq!(turn.answer.as_deref(), Some(ANSWER));
    assert!(
        turn.failure_detail
            .as_deref()
            .is_some_and(|detail| detail.contains("Ask again")),
        "{turn:?}"
    );
    assert_eq!(case.candidates().await, (1, Some(base)));
    case.cleanup().await;
}

// Value: protects=the check refuses only what it should; fails_when=the report check
// refuses every proposal, or a passing one keeps a failure detail; why_new=the control
// for the refusal cases above; seam=report_draft over live Postgres.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_report_whose_proposal_passes_the_daemon_check_writes_the_next_candidate() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;
    let text = definition("0 9 * * *");

    let captured = Capture::install();
    case.report(&processed(&taken, Some(proposal(&taken, &text))))
        .await
        .expect("the report settles");
    let refusals = captured
        .events()
        .iter()
        .filter(|event| event.field("event") == Some(REFUSED))
        .count();
    drop(captured);

    assert_eq!(refusals, 0, "nothing was refused");
    let (count, head) = case.candidates().await;
    assert_eq!(count, 2, "candidate revision 2 is written");
    let turn = case.turn(&taken.turn_id()).await;
    assert_eq!(turn.status, "completed", "{turn:?}");
    assert_eq!(turn.candidate, head, "the turn names the new head");
    assert_eq!(turn.failure_detail, None, "{turn:?}");
    case.cleanup().await;
}
