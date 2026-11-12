#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the daemon's restriction set is the manifest's"
)]

use af_core::test_util::trace::Capture;
use af_wire::report::RenewRequest;
use tracing::Level;

use super::rows::Found;
use super::verdict::{Judged, Verdict, log_refused, refusal_of};
use super::{FALLBACK_RENEWALS_IN_FLIGHT, Renewals, funds_due, in_order};
use crate::lease::test_dead;

#[test]
fn funds_are_checked_on_the_first_tick_then_every_third() {
    let due: Vec<bool> = (0..7).map(funds_due).collect();
    assert_eq!(due, [true, false, false, true, false, false, true]);
}

/// A row every check passed, extended with both rows moved.
fn passing() -> Judged {
    Judged {
        lease_id: test_dead::id(1),
        found: true,
        cancelled: Some(false),
        fence_holds: Some(true),
        below_cap: Some(true),
        priced_as_held: Some(true),
        ceilings_current: Some(true),
        over_budget: Some(false),
        host_renews: Some(true),
        admitted: Some(true),
        has_task: Some(false),
        extended_until: Some(jiff_sqlx::Timestamp::from(test_dead::AT)),
        slot_moved: true,
    }
}

#[test]
fn a_row_every_check_passed_extends() {
    assert_eq!(passing().verdict(), Verdict::Extended(test_dead::AT));
}

/// One failed check, and the verdict it must answer.
type Case = (fn(&mut Judged), Verdict);

#[test]
fn each_failed_check_answers_as_the_per_lease_path_would() {
    let cases: [Case; 12] = [
        (|row| row.found = false, Verdict::Lost),
        (|row| row.has_task = Some(true), Verdict::Tasked),
        (|row| row.host_renews = Some(false), Verdict::Lost),
        (|row| row.admitted = None, Verdict::Lost),
        (|row| row.cancelled = Some(true), Verdict::Cancelled),
        (|row| row.fence_holds = Some(false), Verdict::Lost),
        (|row| row.below_cap = Some(false), Verdict::MaxRuntime),
        (|row| row.priced_as_held = Some(false), Verdict::Moved),
        (|row| row.ceilings_current = Some(false), Verdict::Moved),
        (|row| row.over_budget = Some(true), Verdict::OverBudget),
        (|row| row.slot_moved = false, Verdict::Lost),
        (|row| row.extended_until = None, Verdict::Lost),
    ];
    for (index, (fail, expected)) in cases.into_iter().enumerate() {
        let mut row = passing();
        fail(&mut row);
        assert_eq!(row.verdict(), expected, "case {index}");
    }
}

#[test]
fn a_cancelled_lease_that_lost_its_fence_is_still_cancelled() {
    let mut row = passing();
    row.cancelled = Some(true);
    row.fence_holds = Some(false);
    assert_eq!(
        row.verdict(),
        Verdict::Cancelled,
        "the Kill mark is the answer a runner acts on"
    );
}

// A frame the batch cannot read leaves every lease to the per-lease path, so
// in an outage each answers its own datastore fault, which the runner
// retries, and none is answered lost.
#[tokio::test]
async fn a_frame_the_datastore_refuses_answers_every_lease_retry() {
    let _counted = test_dead::CACHE_LOOKUPS.lock().await;
    let leases = vec![
        (test_dead::id(1), RenewRequest::default()),
        (test_dead::id(4), RenewRequest::default()),
    ];
    let answers = test_dead::plane()
        .renew_batch(Renewals {
            runner_id: &test_dead::id(9),
            leases,
            funds_due: true,
            now: test_dead::AT,
            kept: std::collections::HashMap::new(),
        })
        .await;
    assert_eq!(answers.len(), 2);
    for answer in answers {
        let fault = answer.expect_err("no datastore, no renewal");
        assert!(
            fault.is_datastore_unavailable(),
            "retried, never lost: {fault}"
        );
    }
}

/// A lease row with a fleet and a lease identifier that cannot be mistaken for
/// each other.
fn found() -> Found {
    Found {
        id: test_dead::id(1),
        status: "active".to_owned(),
        cancelled: false,
        tenant_id: test_dead::id(2),
        workspace_id: test_dead::id(3),
        fleet_id: test_dead::id(4),
        posture: "platform".to_owned(),
        provider: "anthropic".to_owned(),
        model: "claude-opus-5".to_owned(),
        task_id: None,
        created_at: test_dead::AT,
        head_revision_id: None,
        catalogue_version: 1,
    }
}

/// A fleet at its ceiling is logged under the per-lease path's own event, with
/// the identifiers an operator greps for.
#[test]
fn a_fleet_at_its_ceiling_is_logged_as_a_budget_breach() {
    let log = Capture::install();
    let row = found();
    let verdict = Verdict::OverBudget;

    log_refused(&row, &verdict, &refusal_of(&verdict));

    let line = log.only("renew_budget_breach");
    assert_eq!(line.level, Level::WARN);
    assert_eq!(line.field("fleet_id"), Some(row.fleet_id.as_str()));
    assert_eq!(line.field("lease_id"), Some(row.id.as_str()));
    assert_eq!(
        line.field("error_code"),
        Some(refusal_of(&verdict).code().as_str())
    );
}

/// The refusals the per-lease path answers silently are named, each with the
/// verdict that stopped the run.
#[test]
fn a_lost_cancelled_or_capped_lease_is_named_with_its_verdict() {
    let row = found();
    for (verdict, label) in [
        (Verdict::Lost, "lost"),
        (Verdict::Cancelled, "cancelled"),
        (Verdict::MaxRuntime, "max_runtime"),
    ] {
        let log = Capture::install();
        log_refused(&row, &verdict, &refusal_of(&verdict));

        let line = log.only("renew_batch_refused");
        assert_eq!(line.field("verdict"), Some(label));
        assert_eq!(line.field("lease_id"), Some(row.id.as_str()));
        assert_eq!(line.field("fleet_id"), Some(row.fleet_id.as_str()));
        assert_eq!(
            line.field("error_code"),
            Some(refusal_of(&verdict).code().as_str())
        );
    }
}

/// An extension, a rerun and the per-lease path are not refusals, so they say
/// nothing here.
#[test]
fn an_answer_that_is_not_a_refusal_logs_nothing() {
    let log = Capture::install();
    let row = found();
    for verdict in [
        Verdict::Extended(test_dead::AT),
        Verdict::Moved,
        Verdict::Tasked,
    ] {
        log_refused(&row, &verdict, &refusal_of(&verdict));
    }
    assert!(log.events().is_empty(), "{:?}", log.events());
}

/// The fallback walk runs its leases together, never more than the bound at
/// once, and answers in the frame's order whichever finishes first.
#[tokio::test]
async fn the_fallback_walk_is_bounded_and_keeps_the_frames_order() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let leases = FALLBACK_RENEWALS_IN_FLIGHT * 3;
    let running = Arc::new(AtomicUsize::new(0));
    let widest = Arc::new(AtomicUsize::new(0));
    let items: Vec<usize> = (0..leases).collect();

    let answers = in_order(items.clone(), FALLBACK_RENEWALS_IN_FLIGHT, |item| {
        let (running, widest) = (Arc::clone(&running), Arc::clone(&widest));
        async move {
            let together = running.fetch_add(1, Ordering::SeqCst) + 1;
            widest.fetch_max(together, Ordering::SeqCst);
            // Later leases finish sooner, so answers that followed completion
            // would come back reversed.
            for _ in 0..(leases - item) {
                tokio::task::yield_now().await;
            }
            running.fetch_sub(1, Ordering::SeqCst);
            item
        }
    })
    .await;

    assert_eq!(answers, items, "answers come back in the frame's order");
    assert_eq!(
        widest.load(Ordering::SeqCst),
        FALLBACK_RENEWALS_IN_FLIGHT,
        "the walk runs the bound together, and no more"
    );
}
