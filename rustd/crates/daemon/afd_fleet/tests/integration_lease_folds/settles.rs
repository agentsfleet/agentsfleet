//! What a report settles of the messages its run claimed.

use af_core::event::status::PROCESSED;
use afd_admission::Producer;
use afd_fleet::lease::Verdict;

use super::support::{
    Checked, EventRow, STEER_JSON, Settlement, claim_as_holder, report_read_through, send, settled,
    workspace_of,
};
use crate::report_commit::RESPONSE_ACCEPTED;
use crate::report_seed::held;

/// A succeeded report settles what a completed call read, each message with
/// its own closed event carrying the run's answer and naming the run; a
/// message claimed but never read stays for the next run.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_succeeded_report_settles_what_the_run_read() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let read = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    let unread = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    assert_eq!(claim_as_holder(&held, 0).await?.len(), 2);
    let folds = report_read_through(&held, read.sequence, Verdict::Succeeded).await?;
    assert_eq!(folds.closed.len(), 1);
    assert_eq!(folds.read_through, read.sequence);
    assert_eq!(
        settled(&held, &read.id).await?,
        Settlement {
            settled: true,
            event: Some(EventRow {
                status: PROCESSED.to_owned(),
                response_text: Some(RESPONSE_ACCEPTED.to_owned()),
                resumes_event_id: Some(held.event_id.clone()),
            }),
        }
    );
    assert_eq!(
        settled(&held, &unread.id).await?,
        Settlement::UNREAD,
        "no call read it"
    );
    held.fixtures.cleanup().await;
    Ok(())
}

/// A failed run's folds settle nothing: each message runs again, in order.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_failed_report_settles_no_fold() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let folded = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    assert_eq!(claim_as_holder(&held, 0).await?.len(), 1);
    let failed = Verdict::Failed {
        class: None,
        detail: "the provider hung up",
    };
    let folds = report_read_through(&held, folded.sequence, failed).await?;
    assert!(folds.closed.is_empty(), "{folds:?}");
    assert_eq!(settled(&held, &folded.id).await?, Settlement::UNREAD);
    held.fixtures.cleanup().await;
    Ok(())
}

/// A run whose calls read none of the messages it claimed settles none of
/// them, and still answers how far it read: they stay for the next run.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_report_that_read_before_its_claims_settles_none() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let claimed = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    assert_eq!(claim_as_holder(&held, 0).await?.len(), 1);
    let before = claimed.sequence - 1;
    let folds = report_read_through(&held, before, Verdict::Succeeded).await?;
    assert!(folds.closed.is_empty(), "{folds:?}");
    assert_eq!(folds.read_through, before);
    assert_eq!(settled(&held, &claimed.id).await?, Settlement::UNREAD);
    held.fixtures.cleanup().await;
    Ok(())
}

/// A burst settles in the same three statements as one message: every
/// message the run read gets its own closed event, past one claim's cap.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_burst_past_one_claim_settles_whole() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let burst = usize::from(af_wire::fold::FOLDS_PER_CLAIM_MAX) + 8;
    let mut sent = Vec::new();
    for _ in 0..burst {
        sent.push(send(&held, &workspace, Producer::Steer, STEER_JSON).await?);
    }
    let first = claim_as_holder(&held, 0).await?;
    let through = first
        .last()
        .ok_or("the first claim took messages")?
        .sequence;
    let rest = claim_as_holder(&held, through).await?;
    assert_eq!(first.len() + rest.len(), burst);
    let last = sent.last().ok_or("a burst was sent")?;
    let folds = report_read_through(&held, last.sequence, Verdict::Succeeded).await?;
    assert_eq!(folds.closed.len(), burst);
    for message in &sent {
        let settlement = settled(&held, &message.id).await?;
        let event = settlement.event.ok_or("each message has its own event")?;
        assert!(settlement.settled && event.status == PROCESSED, "{event:?}");
    }
    held.fixtures.cleanup().await;
    Ok(())
}
