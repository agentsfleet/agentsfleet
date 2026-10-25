//! Admission transfers the current request while history stays private in the daemon.

use super::{Fixture, claim, fixture, funded, mint, now};
use afd_authoring::types::{Draft, SendMessage, Source};
use afd_wire::authoring::{ContextRole, ENVELOPE_BYTES_MAX};

async fn blank(f: &Fixture) -> Draft {
    let mut request = f.request(mint(), Some("First question"));
    request.source = Source::Blank;
    f.drafts
        .create(f.access(), request, now())
        .await
        .expect("blank chat queued")
        .draft
}

async fn complete(f: &Fixture, draft: &Draft, answer: &str) {
    sqlx::query("UPDATE authoring.messages SET status='completed',answer_text=$2 WHERE draft_id=$1::uuid AND sequence=1")
        .bind(draft.id.as_str()).bind(answer).execute(&mut *f.connection().await)
        .await.expect("saved prior assistant answer");
}

async fn send(f: &Fixture, draft: &Draft) {
    f.drafts
        .send(
            f.access(),
            &draft.id,
            SendMessage {
                message: "Next question".into(),
                operation_id: mint(),
            },
            now(),
        )
        .await
        .expect("next question queued");
}

// Value: protects=admission keeps authoritative history while transferring only current work;
// fails_when=prior answers disappear or a blank draft invents a candidate;
// why_new=previous admission only examined the first message with an existing definition;
// seam=real Postgres; no_claim=live provider chat.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_blank_draft_keeps_completed_history_in_the_daemon_without_a_candidate() {
    let f = funded(1).await;
    let draft = blank(&f).await;
    complete(&f, &draft, "Earlier answer").await;
    send(&f, &draft).await;
    let lease = claim(&f, &draft).await.expect("second message admitted");
    assert!(lease.candidate.is_none());
    let actual: Vec<_> = lease
        .messages
        .iter()
        .map(|m| (matches!(m.role, ContextRole::Assistant), m.text.as_str()))
        .collect();
    assert_eq!(actual, [(false, "Next question")]);
    let original: (String, String) = sqlx::query_as("SELECT user_text,answer_text FROM authoring.messages WHERE draft_id=$1::uuid AND sequence=1")
        .bind(draft.id.as_str()).fetch_one(&mut *f.connection().await).await.expect("authoritative original exchange");
    assert_eq!(original, ("First question".into(), "Earlier answer".into()));
    f.cleanup().await;
}

// Value: protects=saved history outgrows the transfer envelope without blocking admission;
// fails_when=history size is mistaken for the current invocation size or old rows are deleted;
// why_new=the current-request test does not cross the former row and byte caps;
// seam=real Postgres fixture rows; no_claim=live chat history.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn saved_history_over_the_old_row_or_transfer_bound_remains_available_for_paged_compaction() {
    for rows in [2_i64, 65] {
        let f = funded(1).await;
        let draft = blank(&f).await;
        let answer = if rows == 2 {
            "a".repeat(ENVELOPE_BYTES_MAX)
        } else {
            "Earlier answer".into()
        };
        complete(&f, &draft, &answer).await;
        if rows == 65 {
            for _ in 2..65 {
                send(&f, &draft).await;
            }
            sqlx::query("UPDATE authoring.messages SET status='completed' WHERE draft_id=$1::uuid")
                .bind(draft.id.as_str())
                .execute(&mut *f.connection().await)
                .await
                .expect("completed owned history");
        }
        send(&f, &draft).await;
        let lease = claim(&f, &draft)
            .await
            .expect("history does not block admission");
        assert_eq!(lease.messages.len(), 1);
        assert_eq!(
            lease.messages.first().expect("current request").text,
            "Next question"
        );
        let saved: (i64, String) = sqlx::query_as("SELECT count(*)::bigint,min(answer_text) FROM authoring.messages WHERE draft_id=$1::uuid")
            .bind(draft.id.as_str()).fetch_one(&mut *f.connection().await).await.expect("complete durable history");
        assert_eq!(saved, (rows, answer));
        assert_eq!(fixture::attempts(&f).await, 1);
        f.cleanup().await;
    }
}
