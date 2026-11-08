//! A ledger that cannot be read refuses the claim before any attempt exists,
//! and the review's wallet check says the same.

use super::fixture::{attempts, funded, queue};
use super::mint;
use af_core::error_code;
use af_crypto::secret::Kek;
use afd_authoring::Setup;
use sqlx::Connection as _;
use std::sync::Arc;

/// An unsettled line no `BIGINT` can sum with a second like it.
const OVERFLOWING_LINE: &str = "INSERT INTO billing.usage_lines \
     (id,work_type,work_id,seq,tenant_id,workspace_id,provider,model,posture,requested_at, \
      input_token_count,cached_input_token_count,cache_write_token_count, \
      cache_write_5m_token_count,cache_write_1h_token_count,output_token_count,runtime_ms, \
      unpriced_token_count,charged_nanos,created_at,updated_at) \
     VALUES($1,'fixture',$1,0,$2,$3,'fixture','fixture','platform',now(), \
      0,0,0,0,0,0,0,0,9223372036854775807,now(),now())";

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_ledger_that_cannot_sum_its_unsettled_lines_refuses_the_claim_and_the_review() {
    let f = funded(1).await;
    let mut connection = f.connection().await;
    for _ in 0..2 {
        sqlx::query(OVERFLOWING_LINE)
            .bind(mint())
            .bind(&f.tenant)
            .bind(&f.workspace)
            .execute(&mut *connection)
            .await
            .expect("a corrupt unsettled line");
    }
    drop(connection);
    let draft = queue(&f, "Claim against a broken ledger").await;
    let refused = f
        .drafts
        .claim_queued(&f.runner, &draft.id)
        .await
        .expect_err("an unreadable balance is no refusal to record");
    assert_eq!(refused.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert_eq!(attempts(&f).await, 0, "no attempt was issued");
    let key = Kek::from_hex("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
        .expect("fixture key");
    let review = Setup::new(f.drafts.clone(), Arc::new(key))
        .read_draft(f.access(), &draft.id)
        .await
        .expect_err("the wallet check cannot read the balance");
    assert_eq!(review.code(), error_code::INTERNAL_OPERATION_FAILED);
    // The corrupt lines are on the shared database and name no fleet or
    // attempt the cleanup would reach: the owner removes them here, since the
    // runtime role may not delete usage lines.
    let mut owner = sqlx::PgConnection::connect(&f.owner_url())
        .await
        .expect("an owner connection");
    sqlx::query("DELETE FROM billing.usage_lines WHERE tenant_id = $1")
        .bind(&f.tenant)
        .execute(&mut owner)
        .await
        .expect("the corrupt lines are removed");
    f.cleanup().await;
}
