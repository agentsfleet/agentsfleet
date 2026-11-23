//! What a setup run's report refuses, and what its assignment and settlement
//! say: a report naming another run or turn writes nothing, a report whose
//! thread moved on or whose run vanished still logs the request lines the
//! runner spent, and a placement choice that moved under the pass assigns
//! nothing.

use std::borrow::Cow;

use af_core::error_code;
use af_core::test_util::trace::Capture;
use af_wire::report::ReportRequest;
use af_wire::run::AcceptedKinds;
use afd_state::sql::RUN_STATUS_ACTIVE;

use super::renewal_refusals::{INPUT_TOKENS, one_request, refused};
use super::{Case, Taken, processed};

/// The event a draft run's committed assignment logs.
const ASSIGNED: &str = "setup_run_assigned";

/// The event a draft run's committed report logs.
const SETTLED: &str = "setup_run_settled";

/// `taken`'s processed report, carrying the one request [`one_request`] holds.
fn with_request(taken: &Taken) -> ReportRequest<'_> {
    ReportRequest {
        requests: one_request().requests,
        ..processed(taken, None)
    }
}

// Value: protects=a runner's report for a run it does not hold, or for another
// turn, writes nothing; fails_when=report_draft stops scoping by runner, or the
// turn compare is dropped and another message is answered; why_new=both
// not-found arms had no case; seam=report_draft over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_setup_report_for_another_run_or_turn_is_not_found_and_writes_nothing() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;
    let stranger = afd_db::test_util::mint_id().to_string();

    let other_run = case
        .report(&ReportRequest {
            run_id: Cow::Borrowed(&stranger),
            ..processed(&taken, None)
        })
        .await;
    let other_turn = case
        .report(&ReportRequest {
            turn_id: Cow::Borrowed(&stranger),
            ..processed(&taken, None)
        })
        .await;

    assert_eq!(refused(other_run), error_code::RUN_NOT_FOUND);
    assert_eq!(refused(other_turn), error_code::RUN_NOT_FOUND);
    let turn = case.turn(&taken.turn_id()).await;
    assert_eq!(turn.status, "running", "{turn:?}");
    assert_eq!(turn.answer, None, "no answer written");
    assert_eq!(case.runs().await, [(1, RUN_STATUS_ACTIVE.to_owned())]);
    case.cleanup().await;
}

// Value: protects=a run whose thread fence moved past it cannot settle, and the
// usage it reported is still billed; fails_when=the settlement's fence compare is
// dropped, its refusal reads as lost, or the refused report skips its request
// lines; why_new=the moved-fence arm and the refused report's lines had no case;
// seam=report_draft over live Postgres, the draft's thread fence advanced.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_setup_report_whose_thread_fence_moved_on_is_stale_and_its_lines_are_logged() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;
    case.alter(
        "UPDATE thread.threads SET fencing_token = fencing_token + 1 WHERE id = $1",
        &case.draft.id,
    )
    .await;

    let answer = case.report(&with_request(&taken)).await;

    assert_eq!(refused(answer), error_code::RUN_STALE_FENCING_TOKEN);
    assert_eq!(
        case.model_lines(&taken.run_id()).await,
        [(0, i64::from(INPUT_TOKENS))],
        "the request the runner spent is owed"
    );
    let turn = case.turn(&taken.turn_id()).await;
    assert_eq!(turn.answer, None, "{turn:?}");
    assert_eq!(case.runs().await, [(1, RUN_STATUS_ACTIVE.to_owned())]);
    case.cleanup().await;
}

// Value: protects=a run removed while its report waits on it is refused as lost,
// never settled or answered as a replay; fails_when=a missing run row reads as
// settled, or the refusal drops the request lines; why_new=the vanished-run arm had
// no case; seam=report_draft over live Postgres, the run row deleted while held.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_setup_run_removed_while_its_report_waits_is_lost() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;
    let run = taken.run_id();
    let request = with_request(&taken);

    // Boxed: the report's state would otherwise ride inside the race's future,
    // past the large-future bound.
    let answer = Box::pin(case.while_held(
        &run,
        "DELETE FROM thread.runs WHERE id = $1",
        Box::pin(case.report(&request)),
    ))
    .await;

    assert_eq!(refused(answer), error_code::RUN_LOST);
    assert_eq!(case.model_lines(&run).await, [(0, i64::from(INPUT_TOKENS))]);
    let turn = case.turn(&taken.turn_id()).await;
    assert_eq!(turn.answer, None, "{turn:?}");
    case.cleanup().await;
}

// Value: protects=an operator can follow one setup run from assignment to
// settlement by draft and run; fails_when=either event loses a field, or carries
// the answer's text; why_new=neither event's fields had a case; seam=the production
// draft assignment and report_draft over live Postgres, read through the capture.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_setup_runs_assignment_and_settlement_are_logged_once_each() {
    let case = Case::queued().await;

    let captured = Capture::install();
    let taken = case.take_on_host().await;
    let assigned = captured.only(ASSIGNED);
    case.report(&processed(&taken, None))
        .await
        .expect("the run settles");
    let settled = captured.only(SETTLED);
    drop(captured);

    assert_eq!(assigned.level, tracing::Level::INFO);
    assert_eq!(assigned.field("draft_id"), Some(case.draft.id.as_str()));
    assert_eq!(assigned.field("run_id"), Some(taken.run.as_str()));
    assert_eq!(
        assigned.field("runner_id"),
        Some(case.running.runner.as_str())
    );
    assert_eq!(assigned.field("fencing_token"), Some("1"));
    assert_eq!(settled.level, tracing::Level::INFO);
    assert_eq!(settled.field("draft_id"), Some(case.draft.id.as_str()));
    assert_eq!(settled.field("outcome"), Some("processed"));
    assert_eq!(settled.field("tokens"), Some("0"));
    assert!(
        settled
            .fields
            .values()
            .all(|value| !value.contains(super::ANSWER)),
        "no answer text reaches the log: {settled:?}"
    );
    case.cleanup().await;
}

// Value: protects=a pass whose placement choice moved under it assigns no setup
// run and leaves the turn for the next poll; fails_when=the draft path drops the
// ticket's revision check and places past a concurrent placement; why_new=the
// busy arm had no case; seam=assign_draft over live Postgres, the host's run
// version advanced after its ticket was read.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_setup_pass_whose_placement_choice_moved_assigns_nothing() {
    let case = Case::queued().await;
    let fixtures = &case.running.fixtures;
    let ticket = afd_runner::Runners::new(
        fixtures.database.clone(),
        af_crypto::entropy::Entropy::new(),
    )
    .assignment_ticket(&case.running.runner, AcceptedKinds::ALL)
    .await
    .expect("the ticket reads")
    .expect("the fixture host has a placement choice");
    case.alter(
        "UPDATE runner.runners SET run_version = run_version + 1 WHERE id = $1",
        &case.running.runner,
    )
    .await;

    let placed = fixtures
        .plane()
        .assign_draft(&case.drafts, &case.running.runner, ticket, case.running.now)
        .await
        .expect("a moved choice is an answer, not a fault");

    assert!(placed.is_none(), "nothing is placed on a moved choice");
    assert_eq!(case.runs().await, Vec::new(), "no run written");
    let turn = case.turn_on_head().await;
    assert_eq!(turn.status, "queued", "the turn waits: {turn:?}");
    let taken = case.take_on_host().await;
    assert_eq!(taken.fence, 1, "the next pass places it");
    case.cleanup().await;
}

// Value: protects=a host whose one worker is busy takes no setup run, and the
// pass's thread and turn writes roll back so the turn waits at its first fence;
// fails_when=the guarded run insert stops counting the host's census, or a refused
// insert commits the thread it took; why_new=the full-host arm had no case;
// seam=the production poll over live Postgres, the fixture's runtime run still live.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_setup_turn_waits_while_its_host_has_no_free_worker() {
    let running = super::Running::create_isolated().await;
    let seat = super::Seat::create(&running).await;
    let drafts = super::Seat::drafts(&running.fixtures.database);
    let draft = seat.queue(&drafts).await;
    let case = Case {
        running,
        seat,
        drafts,
        draft,
    };

    assert!(
        case.take(&case.running.runner).await.is_none(),
        "a full host takes no setup run"
    );
    assert_eq!(case.runs().await, Vec::new(), "no run written");
    let turn = case.turn_on_head().await;
    assert_eq!(turn.status, "queued", "the turn waits: {turn:?}");

    super::expire(&case.running).await;
    let taken = case.take_on_host().await;
    assert_eq!(
        taken.fence, 1,
        "the refused pass left the thread's fence unmoved"
    );
    case.cleanup().await;
}
