//! Rechecks apply to already-open streams, including after a first delivered frame.

use super::fixture::Open;
use crate::harness::send;
use http::{Method, StatusCode};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn private_stream_omits_text_and_ends_after_identity_only_deletion() {
    let mut open = Open::create().await;
    let frame = open.frame().await;
    assert!(frame.contains("event: message.accepted"), "{frame}");
    assert!(frame.contains("\"sequence\":\"1\""));
    assert!(!frame.contains("Investigate checkout"));
    // Consume the current candidate hint so the next read waits for a real change.
    assert!(open.frame().await.contains("event: candidate.changed"));
    let deleted = send(
        &open.fixture.router(),
        Method::DELETE,
        &open.item,
        Some(&open.fixture.token),
        "",
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let frame = open.frame().await;
    assert!(frame.contains("event: draft.deleted"), "{frame}");
    assert!(frame.contains(&open.draft));
    assert!(!frame.contains("candidate_id"));
    assert!(!frame.contains("sequence"));
    open.closed().await;
    open.finish().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn revoking_the_person_closes_an_existing_private_stream() {
    let mut open = Open::create().await;
    assert!(open.frame().await.contains("event: message.accepted"));
    assert!(open.frame().await.contains("event: candidate.changed"));
    sqlx::query("DELETE FROM core.users WHERE id=$1::uuid")
        .bind(open.fixture.owner.as_str())
        .execute(
            &mut *open
                .fixture
                .db
                .acquire()
                .await
                .expect("revocation connection"),
        )
        .await
        .expect("revoke the authenticated person");
    open.closed().await;
    open.finish().await;
}
