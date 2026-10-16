//! Stop racing a send or a claim never doubles a lease or drops an obligation,
//! and the report a runner retained across the Stop settles exactly once.

use super::{Fixture, claim, mint, now, queue};
use afd_authoring::{
    Claimed,
    types::{Draft, MessageQuery, MessageState, SendMessage},
};
use afd_wire::authoring::{Accounting, Knowledge, Meter, Outcome, Report};

/// What one millisecond of reported runtime debits at the fixture rate.
const RUNTIME_DEBIT_NANOS: i64 = 100;

/// `(active attempts, their reserved nanos)` for one draft.
async fn obligations(f: &Fixture, draft: &Draft) -> (i64, i64) {
    sqlx::query_as(
        "SELECT count(*), COALESCE(sum(reserved_nanos),0)::bigint FROM fleet.authoring_attempts \
         WHERE draft_id=$1::uuid AND status='active'",
    )
    .bind(draft.id.as_str())
    .fetch_one(&mut *f.connection().await)
    .await
    .expect("owned obligations")
}

async fn balance(f: &Fixture) -> i64 {
    sqlx::query_scalar("SELECT balance_nanos FROM billing.tenant_wallet WHERE tenant_id=$1::uuid")
        .bind(f.tenant.as_str())
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("owned wallet")
}

async fn states(f: &Fixture, draft: &Draft) -> Vec<(String, String, Option<String>)> {
    f.drafts
        .messages(
            f.access(),
            &draft.id,
            MessageQuery {
                limit: 20,
                starting_after: None,
            },
        )
        .await
        .expect("creator reload")
        .items
        .into_iter()
        .map(|m| {
            let state = match m.state {
                MessageState::Queued => "queued",
                MessageState::Running => "running",
                MessageState::Cancelled => "cancelled",
                MessageState::Completed => "completed",
                MessageState::Failed => "failed",
                MessageState::TimedOut => "timed_out",
            };
            (
                format!("{}:{}", m.sequence, m.role),
                state.to_owned(),
                m.text,
            )
        })
        .collect()
}

/// The terminal report a runner that finished before seeing the Stop retains.
pub(super) fn finished(lease: &Claimed) -> Report {
    Report::Terminal {
        accounting: Accounting {
            fence: lease.fence.clone(),
            deletion_generation: lease.deletion_generation.clone(),
            report_id: mint().to_string(),
            outcome: Outcome::Completed,
            failure_reason: None,
            usage: Meter {
                knowledge: Knowledge::Known,
                calls: 0,
                input_tokens: Some(0),
                output_tokens: Some(0),
                runtime_ms: 1,
                provider_cost_nanos: None,
            },
        },
        answer: Some("Late private answer".into()),
    }
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn send_racing_stop_keeps_one_lease_and_the_retained_report_settles_once() {
    let f = super::funded(2).await;
    let draft = queue(&f, "First").await;
    let first = claim(&f, &draft).await.expect("first request admitted");
    let held = obligations(&f, &draft).await;
    assert_eq!(held.0, 1);
    assert!(held.1 > 0, "admission reserved a bound");
    let second = SendMessage {
        message: "Second".into(),
        operation_id: mint(),
    };
    let (sent, stopped, raced) = tokio::join!(
        f.drafts.send(f.access(), &draft.id, second, now()),
        f.drafts.cancel(f.access(), &draft.id, 1, now()),
        claim(&f, &draft),
    );
    assert_eq!(sent.expect("send commits around the Stop").sequence, "2");
    assert_eq!(stopped.expect("Stop commits").cancelled_through, "1");
    assert!(
        raced.is_none(),
        "an outstanding attempt blocks a second lease"
    );
    assert_eq!(obligations(&f, &draft).await, held, "Stop keeps the bound");
    assert!(claim(&f, &draft).await.is_none(), "still one live attempt");
    let report = finished(&first);
    let before = balance(&f).await;
    let lease = afd_core::id::Uuid7::parse(&first.lease_id).expect("lease id");
    for replayed in [false, true] {
        let ack = f
            .drafts
            .report(&f.runner, &lease, report.clone())
            .await
            .expect("a retained report is answered, not left to retry");
        assert!(ack.settled, "the spool removes only a settled entry");
        assert_eq!(ack.replayed, replayed);
    }
    assert_eq!(
        balance(&f).await,
        before - RUNTIME_DEBIT_NANOS,
        "debited once"
    );
    assert_eq!(obligations(&f, &draft).await, (0, 0));
    let after = states(&f, &draft).await;
    assert!(
        after
            .iter()
            .all(|(_, _, text)| text.as_deref() != Some("Late private answer"))
    );
    assert!(after.contains(&("1:user".into(), "cancelled".into(), Some("First".into()))));
    assert!(after.contains(&("2:user".into(), "queued".into(), Some("Second".into()))));
    claim_racing_stop(&f, &draft).await;
    f.cleanup().await;
}

/// A queued request is either stopped before admission or admitted and then
/// stopped; both leave at most one attempt holding its whole bound.
async fn claim_racing_stop(f: &Fixture, draft: &Draft) {
    let (claimed, stopped) = tokio::join!(
        claim(f, draft),
        f.drafts.cancel(f.access(), &draft.id, 2, now())
    );
    assert_eq!(stopped.expect("Stop commits").cancelled_through, "2");
    let (live, reserved) = obligations(f, draft).await;
    assert_eq!(live, i64::from(claimed.is_some()));
    assert_eq!(
        reserved > 0,
        claimed.is_some(),
        "an admitted attempt keeps its bound"
    );
    assert!(claim(f, draft).await.is_none(), "nothing left to admit");
    let second = states(f, draft).await;
    assert!(second.contains(&("2:user".into(), "cancelled".into(), Some("Second".into()))));
    if let Some(lease) = claimed {
        let id = afd_core::id::Uuid7::parse(&lease.lease_id).expect("lease id");
        let ack = f
            .drafts
            .report(&f.runner, &id, finished(&lease))
            .await
            .expect("report after Stop");
        assert!(ack.settled);
        assert_eq!(obligations(f, draft).await, (0, 0));
    }
}
