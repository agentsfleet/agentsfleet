//! Native placement pins a single accepted request and never discloses credentials.

use super::{claim, funded, mint, now, queue};
use afd_authoring::types::SendMessage;
use afd_core::timing::LEASE_TTL;
use jiff::SignedDuration;
use sha2::{Digest as _, Sha256};

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_model_output_ceiling_can_equal_its_context_without_excluding_every_message() {
    let f = funded(1).await;
    sqlx::query("UPDATE model.platform_models SET context_cap_tokens=32768,inference=jsonb_set(inference,'{max_output_tokens}','32768') WHERE id=$1::uuid")
        .bind(f.catalogue.first().expect("owned model").as_str()).execute(&mut *f.connection().await).await.expect("configured model ceilings");
    let draft = queue(&f, "Investigate").await;
    let lease = claim(&f, &draft)
        .await
        .expect("the input leaves room for a completion");
    assert_eq!(
        lease.limits.output_per_turn, 32_768,
        "the persisted turn ceiling remains authoritative"
    );
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn lease_pins_oldest_message_model_candidate_and_only_hashes_its_capability() {
    let f = funded(2).await;
    let draft = queue(&f, "First private question").await;
    f.drafts
        .send(
            f.access(),
            &draft.id,
            SendMessage {
                save: false,
                message: "Second private question".into(),
                operation_id: mint(),
            },
            now(),
        )
        .await
        .expect("second queued message");
    let lease = claim(&f, &draft).await.expect("first placed request");
    assert_eq!(lease.sequence, "1");
    assert_eq!(lease.messages.len(), 1);
    assert_eq!(
        lease.messages.first().expect("current user context").text,
        "First private question"
    );
    assert_eq!(
        lease.candidate.as_ref().expect("native candidate").id,
        draft.candidate.expect("source candidate").id
    );
    assert_eq!(lease.model.reference, f.model.to_string());
    assert_eq!(lease.model_access.token.len(), 43);
    let saved: (String, i64, jiff_sqlx::Timestamp, jiff_sqlx::Timestamp) = sqlx::query_as(
        "SELECT model_token_hash, reserved_nanos, expires_at, lease_expires_at \
         FROM authoring.attempts WHERE id = $1::uuid",
    )
    .bind(&lease.attempt_id)
    .fetch_one(&mut *f.connection().await)
    .await
    .expect("durable placement");
    let (token_hash, reserved_nanos, expires_at, lease_expires_at) = saved;
    assert_eq!(
        token_hash,
        hex::encode(Sha256::digest(lease.model_access.token.as_bytes()))
    );
    assert_eq!(reserved_nanos, 30_000_000);
    // Both deadlines come from one clock reading: the five-minute wall bound
    // sits one lease lifetime past the lease.
    let lifetime = SignedDuration::try_from(LEASE_TTL).expect("a lease lifetime fits");
    assert_eq!(
        expires_at
            .to_jiff()
            .duration_since(lease_expires_at.to_jiff()),
        SignedDuration::from_mins(5) - lifetime
    );
    let debug = format!("{lease:?} {:?}", lease.model_access);
    assert!(!debug.contains(&lease.model_access.token));
    assert!(!debug.contains("First private question"));
    let json = serde_json::to_string(&lease).expect("machine payload");
    assert!(!json.contains("api_key") && !json.contains("secrets_map"));
    f.cleanup().await;
}
