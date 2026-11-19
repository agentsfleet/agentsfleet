//! Native placement pins a single accepted request and never discloses credentials.

use super::{claim, funded, mint, now, queue};
use af_core::id::Uuid7;
use af_core::timing::RUN_TTL;
use afd_setup::types::SendMessage;
use jiff::SignedDuration;
use sha2::{Digest as _, Sha256};

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_model_output_ceiling_can_equal_its_context_without_excluding_every_message() {
    let f = funded(1).await;
    sqlx::query("UPDATE model.platform_models SET context_cap_tokens=32768,inference=jsonb_set(inference,'{max_output_tokens}','32768') WHERE id=$1")
        .bind(f.catalogue.first().expect("owned model")).execute(&mut *f.connection().await).await.expect("configured model ceilings");
    let draft = queue(&f, "Investigate").await;
    let run = claim(&f, &draft)
        .await
        .expect("the input leaves room for a completion");
    assert_eq!(
        run.limits.output_per_turn, 32_768,
        "the persisted turn ceiling remains authoritative"
    );
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn run_pins_oldest_message_model_candidate_and_only_hashes_its_capability() {
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
    let run = claim(&f, &draft).await.expect("first placed request");
    assert_eq!(run.sequence, "1");
    assert_eq!(run.messages.len(), 1);
    assert_eq!(
        run.messages.first().expect("current user context").text,
        "First private question"
    );
    assert_eq!(
        run.candidate.as_ref().expect("native candidate").id,
        draft.candidate.expect("source candidate").id
    );
    assert_eq!(run.model.reference, f.model.to_string());
    assert_eq!(run.model_access.token.len(), 43);
    let saved: (String, i64, jiff_sqlx::Timestamp, jiff_sqlx::Timestamp) = sqlx::query_as(
        "SELECT model_token_hash, reserved_nanos, expires_at, run_expires_at \
         FROM authoring.attempts WHERE id = $1",
    )
    .bind(Uuid7::parse(&run.attempt_id).expect("the run names its attempt"))
    .fetch_one(&mut *f.connection().await)
    .await
    .expect("durable placement");
    let (token_hash, reserved_nanos, expires_at, run_expires_at) = saved;
    assert_eq!(
        token_hash,
        hex::encode(Sha256::digest(run.model_access.token.as_bytes()))
    );
    assert_eq!(reserved_nanos, 30_000_000);
    // Both deadlines come from one clock reading: the five-minute wall bound
    // sits one run lifetime past the run.
    let lifetime = SignedDuration::try_from(RUN_TTL).expect("a run lifetime fits");
    assert_eq!(
        expires_at
            .to_jiff()
            .duration_since(run_expires_at.to_jiff()),
        SignedDuration::from_mins(5) - lifetime
    );
    let debug = format!("{run:?} {:?}", run.model_access);
    assert!(!debug.contains(&run.model_access.token));
    assert!(!debug.contains("First private question"));
    let json = serde_json::to_string(&run).expect("machine payload");
    assert!(!json.contains("api_key") && !json.contains("secrets_map"));
    f.cleanup().await;
}
