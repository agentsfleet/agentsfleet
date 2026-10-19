//! Admission preserves completed history and refuses oversized saved conversations.

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

// Value: protects=the admitted envelope keeps user and assistant history in order;
// fails_when=prior answers disappear or a blank draft invents a candidate;
// why_new=previous admission only examined the first message with an existing definition;
// seam=real Postgres; no_claim=live provider chat.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_blank_draft_keeps_completed_assistant_context_without_a_candidate() {
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
    assert_eq!(
        actual,
        [
            (false, "First question"),
            (true, "Earlier answer"),
            (false, "Next question"),
        ]
    );
    f.cleanup().await;
}

// Value: protects=oversized saved history is rejected whole before placement;
// fails_when=admission truncates history, ignores its envelope cap or records an obligation;
// why_new=the existing context test covers tokenizer exhaustion but not rows/bytes;
// seam=real Postgres fixture rows; no_claim=live chat history.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn saved_history_over_the_row_or_envelope_bound_is_a_durable_token_failure() {
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
        assert!(claim(&f, &draft).await.is_none());
        let saved: (String, String) = sqlx::query_as("SELECT status,failure_reason FROM authoring.messages WHERE draft_id=$1::uuid ORDER BY sequence DESC LIMIT 1")
            .bind(draft.id.as_str()).fetch_one(&mut *f.connection().await).await.expect("latest failed message");
        assert_eq!(saved, ("failed".into(), "context_too_large".into()));
        assert_eq!(fixture::attempts(&f).await, 0);
        f.cleanup().await;
    }
}
