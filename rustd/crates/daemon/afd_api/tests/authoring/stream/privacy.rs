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
    sqlx::query("DELETE FROM identity.users WHERE id=$1::uuid")
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

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn idle_private_stream_sends_an_observable_heartbeat_and_still_rechecks_access() {
    let mut open = Open::create().await;
    assert!(open.frame().await.contains("event: message.accepted"));
    assert!(open.frame().await.contains("event: candidate.changed"));
    let bytes = tokio::time::timeout(
        afd_sse::HEARTBEAT_INTERVAL + super::DELIVERY,
        open.response.chunk(),
    )
    .await
    .expect("idle heartbeat arrives before the browser silence deadline")
    .expect("heartbeat socket read")
    .expect("private stream remains open while idle");
    let frame = std::str::from_utf8(&bytes).expect("UTF-8 heartbeat");
    assert!(frame.contains("event: heartbeat\n"), "{frame}");
    assert!(
        frame.contains("data: {\"kind\":\"heartbeat\"}\n"),
        "{frame}"
    );
    assert!(
        !frame.contains(&open.draft),
        "heartbeat carries no private identifiers"
    );
    sqlx::query("DELETE FROM identity.users WHERE id=$1::uuid")
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
        .expect("revoke the authenticated person after heartbeat");
    open.closed().await;
    open.finish().await;
}
