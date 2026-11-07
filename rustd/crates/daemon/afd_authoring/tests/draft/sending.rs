//! A send outside editing, or one a racing writer beat to its key, writes
//! nothing; a page cursor of the wrong kind or range is refused.

use super::{
    fixture::{Fixture, now},
    mint,
    window::{message_id, send},
};
use afd_authoring::types::{DraftState, MessageQuery};
use afd_core::{
    error_code::{AUTHORING_CURSOR, AUTHORING_REPLAY, AUTHORING_STATE},
    id::Uuid7,
    paging::Cursor,
};
use sqlx::{Postgres, Transaction};
use std::time::Duration;

/// How often, and how many times, a proof looks for a send blocked on the
/// racing writer. Their product stays inside the pool's two-second lock limit,
/// which would otherwise refuse the waiting insert first.
const POLL: Duration = Duration::from_millis(10);
const POLLS: usize = 100;

/// Files a receipt for key `$4` in scope `$3` as the racing writer; `$1` id,
/// `$2` workspace, `$5` user, `$6` resource.
const RACER: &str = "\
INSERT INTO authoring.replay_receipts
 (id,workspace_id,user_id,scope,operation_id,request_digest,resource_id,created_at)
VALUES ($1::uuid,$2::uuid,$5::uuid,$3,$4::uuid,'racer',$6::uuid,now())";

/// Backends waiting on a lock held by backend `$1`.
const BLOCKED: &str = "SELECT count(*) FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid))";

async fn draft_with_message(f: &Fixture, key: &Uuid7) -> Uuid7 {
    let request = f.request(key.clone(), Some("Watch checkout"));
    let created = f.drafts.create(f.access(), request, now()).await;
    created.expect("draft with its first message").draft.id
}

async fn messages(f: &Fixture, draft: &Uuid7) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM authoring.messages WHERE draft_id=$1::uuid")
        .bind(draft.as_str())
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("message census")
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_send_to_a_draft_being_saved_replays_its_accepted_key_and_refuses_a_new_one() {
    let f = Fixture::create().await;
    let key = mint();
    let draft = draft_with_message(&f, &key).await;
    sqlx::query("UPDATE authoring.drafts SET status=$2 WHERE id=$1::uuid")
        .bind(draft.as_str())
        .bind(DraftState::Saving.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("a setup reserved the candidate");
    let replayed = send(&f, &draft, &key, "Watch checkout").await;
    assert!(replayed.expect("an accepted key still replays").replayed);
    let refused = send(&f, &draft, &mint(), "Watch billing").await;
    assert_eq!(refused.expect_err("saving").code(), AUTHORING_STATE);
    assert_eq!(messages(&f, &draft).await, 1, "the refusal wrote nothing");
    f.cleanup().await;
}

/// Waits until a backend is blocked on `racer`, then commits it.
async fn commit_once_blocked(f: &Fixture, racer: Transaction<'static, Postgres>, pid: i32) {
    for _ in 0..POLLS {
        let blocked: i64 = sqlx::query_scalar(BLOCKED)
            .bind(pid)
            .fetch_one(&mut *f.connection().await)
            .await
            .expect("lock census");
        if blocked > 0 {
            break;
        }
        tokio::time::sleep(POLL).await;
    }
    racer.commit().await.expect("racing writer commits");
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_key_a_racing_writer_files_first_refuses_the_send_and_writes_nothing() {
    let f = Fixture::create().await;
    let draft = draft_with_message(&f, &mint()).await;
    let first = message_id(&f, &draft, 1).await;
    let scope: String = sqlx::query_scalar(
        "SELECT scope FROM authoring.replay_receipts WHERE resource_id=$1::uuid",
    )
    .bind(&first)
    .fetch_one(&mut *f.connection().await)
    .await
    .expect("the draft's message scope");
    let key = mint();
    let racer = Transaction::begin(f.connection().await, None).await;
    let mut racer = racer.expect("racing writer begins");
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *racer)
        .await
        .expect("racing writer's backend");
    sqlx::query(RACER)
        .bind(mint().as_str())
        .bind(f.workspace.as_str())
        .bind(&scope)
        .bind(key.as_str())
        .bind(f.owner.as_str())
        .bind(mint().as_str())
        .execute(&mut *racer)
        .await
        .expect("racing writer files the key, uncommitted");
    let (sent, ()) = tokio::join!(
        send(&f, &draft, &key, "Watch billing"),
        commit_once_blocked(&f, racer, pid),
    );
    assert_eq!(
        sent.expect_err("the key was filed first").code(),
        AUTHORING_REPLAY
    );
    assert_eq!(
        messages(&f, &draft).await,
        1,
        "the refused send wrote nothing"
    );
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_message_cursor_of_another_kind_or_out_of_range_is_refused() {
    let f = Fixture::create().await;
    let draft = draft_with_message(&f, &mint()).await;
    send(&f, &draft, &mint(), "Watch billing")
        .await
        .expect("second turn");
    let page = |starting_after| MessageQuery {
        limit: 1,
        starting_after,
    };
    let first = f.drafts.messages(f.access(), &draft, page(None)).await;
    let issued = first
        .expect("first page")
        .next_cursor
        .expect("a second turn follows");
    let value = match Cursor::parse(&issued) {
        Ok(Cursor::Text { value, .. }) => Some(value),
        Ok(Cursor::Timestamp { .. }) | Err(_) => None,
    };
    let value = value.expect("message pages issue text cursors");
    let at = now();
    let refused = [
        Cursor::Timestamp {
            at,
            id: "1:0".into(),
        },
        Cursor::Text {
            value: value.clone(),
            id: "0:0".into(),
        },
        Cursor::Text {
            value: value.clone(),
            id: "1:2".into(),
        },
    ];
    for cursor in refused {
        let answer = f
            .drafts
            .messages(f.access(), &draft, page(Some(cursor.to_string())))
            .await;
        assert_eq!(
            answer.expect_err("refused cursor").code(),
            AUTHORING_CURSOR,
            "{cursor}"
        );
    }
    let resumed = Cursor::Text {
        value,
        id: "1:0".into(),
    }
    .to_string();
    let next = f
        .drafts
        .messages(f.access(), &draft, page(Some(resumed)))
        .await;
    let next = next.expect("an in-range cursor resumes");
    assert_eq!(
        next.items.first().map(|item| item.sequence.as_str()),
        Some("2")
    );
    f.cleanup().await;
}
