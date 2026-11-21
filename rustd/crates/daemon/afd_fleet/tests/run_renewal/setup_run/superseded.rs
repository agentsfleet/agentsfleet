//! A stale fence writes nothing: once a lapsed setup run's turn is assigned
//! again at a higher fence, the first holder's late report is refused with
//! `UZ-RUN-005`, and so is a report naming the live run at the old fence.

use af_core::error_code;
use afd_state::sql::{RUN_STATUS_ACTIVE, RUN_STATUS_EXPIRED};

use super::{Case, definition, processed, proposal};

// Value: protects=a superseded holder cannot write a candidate or answer the turn;
// fails_when=the draft report accepts an expired run's report, or skips the fence
// compare; why_new=an expired draft run answered run_lost, not the fence refusal a
// fleet's superseded report gets; seam=the production draft assignment twice, then
// report_draft at the first fence over live Postgres.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_superseded_setup_run_report_is_refused() {
    let case = Case::queued().await;
    let first = case.take_on_host().await;
    assert_eq!(first.fence, 1, "the draft's first run takes fence 1");
    case.lapse(&first.run_id()).await;
    let second = case.take_on_host().await;
    assert_eq!(second.fence, 2, "the lapsed turn is reassigned at fence 2");
    assert_eq!(second.turn, first.turn, "the same turn, run again");
    let text = definition("0 9 * * *");

    let late = case
        .report(&processed(&first, Some(proposal(&first, &text))))
        .await;
    assert!(
        matches!(&late, Err(refused) if refused.code() == error_code::RUN_STALE_FENCING_TOKEN),
        "the first holder's late report: {late:?}"
    );
    let mut stale = processed(&second, Some(proposal(&second, &text)));
    stale.fencing_token = first.fence;
    let fenced = case.report(&stale).await;
    assert!(
        matches!(&fenced, Err(refused) if refused.code() == error_code::RUN_STALE_FENCING_TOKEN),
        "the live run at the old fence: {fenced:?}"
    );

    let (candidates, _head) = case.candidates().await;
    assert_eq!(candidates, 1, "no candidate written");
    let turn = case.turn(&first.turn_id()).await;
    assert_eq!(
        turn.status, "running",
        "the live run still holds it: {turn:?}"
    );
    assert_eq!(turn.answer, None, "no answer written");
    assert_eq!(
        case.runs().await,
        vec![
            (1, RUN_STATUS_EXPIRED.to_owned()),
            (2, RUN_STATUS_ACTIVE.to_owned())
        ],
        "the live run is untouched"
    );
    case.cleanup().await;
}
