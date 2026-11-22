//! An event already terminal keeps its settled result: neither a report's fold
//! nor a direct terminal write reopens it, and each says so once.

use af_core::test_util::trace::Capture;
use af_core::turn::status::PROCESSED;
use afd_fleet::run::Verdict;
use afd_fleet::run::verdict::Terminal;
use afd_turns::Producer;

use super::support::{
    Checked, STEER_JSON, assign_as_holder, report_read_through, send, workspace_of,
};
use crate::report_seed::{Held, held};

/// The event a terminal write that found nothing to close logs.
const SKIPPED: &str = "terminal_write_skipped_nonreceived";

/// The answer an earlier report left on the event.
const EARLIER: &str = "answered by the first report";

/// Closes `turn` as an earlier report's commit would have left it.
async fn answer_earlier(held: &Held, turn: &af_core::id::Uuid7) -> Checked {
    let mut connection = held.fixtures.database.acquire().await?;
    let closed = sqlx::query(
        "UPDATE thread.turns SET status = $2, answer_text = $3, opened_at = now() \
         WHERE id = $1",
    )
    .bind(turn)
    .bind(PROCESSED)
    .bind(EARLIER)
    .execute(&mut *connection)
    .await?;
    assert_eq!(closed.rows_affected(), 1);
    Ok(())
}

/// `turn`'s status and answer.
async fn result_of(held: &Held, turn: &af_core::id::Uuid7) -> Checked<(String, Option<String>)> {
    let mut connection = held.fixtures.database.acquire().await?;
    Ok(
        sqlx::query_as("SELECT status, answer_text FROM thread.turns WHERE id = $1")
            .bind(turn)
            .fetch_one(&mut *connection)
            .await?,
    )
}

// Value: protects=a redelivered report never overwrites a folded message's
// settled answer, and an operator sees how many it skipped; fails_when=the fold's
// close drops its received guard, or the skip goes unsaid; why_new=the skipped-fold
// arm had no case; seam=commit_report over live Postgres, the read message already
// answered when the report settles it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_folded_message_already_answered_keeps_its_result() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let read = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    assert_eq!(assign_as_holder(&held, 0).await?.len(), 1);
    answer_earlier(&held, &read.id).await?;

    let captured = Capture::install();
    let folds = report_read_through(&held, read.sequence, Verdict::Succeeded).await?;
    let skipped = captured.only(SKIPPED);
    drop(captured);

    assert!(folds.closed.is_empty(), "nothing was closed again");
    assert_eq!(skipped.level, tracing::Level::WARN);
    assert_eq!(skipped.field("skipped"), Some("1"));
    assert_eq!(skipped.field("fleet_id"), Some(held.fleet.as_str()));
    assert_eq!(
        result_of(&held, &read.id).await?,
        (PROCESSED.to_owned(), Some(EARLIER.to_owned())),
        "the first answer stands"
    );
    held.fixtures.cleanup().await;
    Ok(())
}

// Value: protects=a terminal write on an event already terminal keeps the settled
// result and is answered as nothing closed; fails_when=the write's received guard
// is dropped, or a skip is treated as a fault; why_new=the skipped single-event arm
// had no case; seam=Runs::mark_terminal over live Postgres, the event closed first.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_terminal_write_on_a_closed_event_closes_nothing() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let sent = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    answer_earlier(&held, &sent.id).await?;

    let captured = Capture::install();
    let mut connection = held.fixtures.database.acquire().await?;
    let closed = held
        .runs
        .mark_terminal(
            &mut connection,
            &held.fleet,
            &sent.id,
            Terminal {
                verdict: Verdict::Succeeded,
                response_text: "a late copy",
                tokens: 0,
                wall_ms: 0,
                tool_calls: None,
            },
        )
        .await?;
    drop(connection);
    let skipped = captured.only(SKIPPED);
    drop(captured);

    assert!(closed.is_none(), "a closed event is not closed again");
    assert_eq!(skipped.level, tracing::Level::WARN);
    assert_eq!(skipped.field("agentsfleet_turn_id"), Some(sent.id.as_str()));
    assert_eq!(skipped.field("fleet_id"), Some(held.fleet.as_str()));
    assert_eq!(
        result_of(&held, &sent.id).await?,
        (PROCESSED.to_owned(), Some(EARLIER.to_owned()))
    );
    held.fixtures.cleanup().await;
    Ok(())
}
