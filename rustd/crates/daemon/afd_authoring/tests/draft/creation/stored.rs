//! A saved fleet whose stored setup cannot be read back is a stored fault,
//! never a missing fleet or a guess at what the row meant.

use super::super::mint;
use super::fixture::Case;
use afd_authoring::types::Start;
use afd_core::{error_code, id::Uuid7};

/// Appends a copy of the fleet's head revision with `$3` as its frozen review
/// and `$4` as its source candidate, and moves the head to it.
const CORRUPT_HEAD: &str = "WITH head AS ( \
         SELECT r.* FROM fleet.revisions r JOIN fleet.fleets f ON f.head_revision_id=r.id \
         WHERE f.id=$1::uuid), \
     appended AS ( \
         INSERT INTO fleet.revisions(id,fleet_id,revision,definition,canonical_digest, \
             policy_semantics,runtime_model_ref,review_digest,review_context, \
             source_candidate_id,created_at) \
         SELECT $2::uuid,fleet_id,revision+1,definition,canonical_digest,policy_semantics, \
             runtime_model_ref,review_digest,COALESCE($3::jsonb,review_context), \
             COALESCE($4::uuid,source_candidate_id),now() FROM head RETURNING id) \
     UPDATE fleet.fleets SET head_revision_id=(SELECT id FROM appended) WHERE id=$1::uuid";

/// A UUID of version 4, which no identifier this daemon mints can be.
const NOT_MINTED: &str = "4b1c6c4e-8f3a-4d2b-9a7e-1f0c2d3e4a5b";

async fn refused_read(c: &Case, fleet: &Uuid7, code: error_code::ErrorCode, why: &str) {
    let refused = c
        .creations
        .read(c.base.access(), fleet)
        .await
        .expect_err(why);
    assert_eq!(refused.code(), code, "{why}");
}

async fn corrupt_head(c: &Case, fleet: &Uuid7, review: Option<&str>, source: Option<&str>) {
    sqlx::query(CORRUPT_HEAD)
        .bind(fleet.as_str())
        .bind(mint().as_str())
        .bind(review)
        .bind(source)
        .execute(&mut *c.base.connection().await)
        .await
        .expect("an appended revision another writer got wrong");
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_saved_fleet_whose_stored_setup_is_unreadable_reads_as_a_stored_fault() {
    let c = Case::create().await;
    let (_, request) = c.request("stored-faults", Start::None).await;
    let fleet = c.save(&mint(), request).await.id;
    let initial: String =
        sqlx::query_scalar("SELECT initial_start::text FROM fleet.fleets WHERE id=$1::uuid")
            .bind(fleet.as_str())
            .fetch_one(&mut *c.base.connection().await)
            .await
            .expect("the saved initial start");
    let set_initial = |value: String| {
        let c = &c;
        let fleet = &fleet;
        async move {
            sqlx::query("UPDATE fleet.fleets SET initial_start=$2::jsonb WHERE id=$1::uuid")
                .bind(fleet.as_str())
                .bind(value)
                .execute(&mut *c.base.connection().await)
                .await
                .expect("rewrite the initial start");
        }
    };
    set_initial(r#"{"kind":"run_later"}"#.to_owned()).await;
    refused_read(
        &c,
        &fleet,
        error_code::INTERNAL_DB_QUERY,
        "an initial start of no known kind",
    )
    .await;
    set_initial(initial).await;
    c.creations
        .read(c.base.access(), &fleet)
        .await
        .expect("the restored row reads");
    corrupt_head(&c, &fleet, None, Some(NOT_MINTED)).await;
    refused_read(
        &c,
        &fleet,
        error_code::INTERNAL_OPERATION_FAILED,
        "a source candidate this daemon never minted",
    )
    .await;
    corrupt_head(&c, &fleet, Some(r#"{"subject":"unknown"}"#), None).await;
    refused_read(
        &c,
        &fleet,
        error_code::INTERNAL_DB_QUERY,
        "a frozen review of no known shape",
    )
    .await;
    c.cleanup().await;
}
