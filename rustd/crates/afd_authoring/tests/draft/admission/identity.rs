//! Native placement pins a single accepted request and never discloses credentials.

use super::{claim, funded, mint, now, queue};
use afd_authoring::types::SendMessage;
use sha2::{Digest as _, Sha256};

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
    let saved: (String, i64, i64) = sqlx::query_as("SELECT capability_hash,reserved_nanos,deadline-created_at FROM fleet.authoring_attempts WHERE id=$1::uuid")
        .bind(&lease.attempt_id).fetch_one(&mut *f.connection().await).await.expect("durable placement");
    assert_eq!(
        saved,
        (
            hex::encode(Sha256::digest(lease.model_access.token.as_bytes())),
            30_000_000,
            300_000
        )
    );
    let debug = format!("{lease:?} {:?}", lease.model_access);
    assert!(!debug.contains(&lease.model_access.token));
    assert!(!debug.contains("First private question"));
    let json = serde_json::to_string(&lease).expect("machine payload");
    assert!(!json.contains("api_key") && !json.contains("secrets_map"));
    f.cleanup().await;
}
