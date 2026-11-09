//! A saved fleet whose stored setup cannot be read back is a stored fault,
//! never a missing fleet or a guess at what the row meant.

use super::super::mint;
use super::fixture::Case;
use af_core::{error_code, id::Uuid7};
use afd_authoring::types::Start;

/// Appends a copy of the fleet's head revision with `$3` as its frozen review
/// and, when `$4` is set, a source candidate this daemon never minted, and
/// moves the head to it.
///
/// The foreign candidate is Postgres's `gen_random_uuid()`, a version-4 UUID,
/// which no identifier this daemon mints can be.
const CORRUPT_HEAD: &str = "WITH head AS ( \
         SELECT r.* FROM fleet.revisions r JOIN fleet.fleets f ON f.head_revision_id=r.id \
         WHERE f.id=$1), \
     appended AS ( \
         INSERT INTO fleet.revisions(id,fleet_id,revision,definition,canonical_digest, \
             policy_semantics,runtime_model_ref,review_digest,review_context, \
             source_candidate_id,created_at) \
         SELECT $2,fleet_id,revision+1,definition,canonical_digest,policy_semantics, \
             runtime_model_ref,review_digest,COALESCE($3::jsonb,review_context), \
             CASE WHEN $4 THEN gen_random_uuid() ELSE source_candidate_id END,now() \
             FROM head RETURNING id) \
     UPDATE fleet.fleets SET head_revision_id=(SELECT id FROM appended) WHERE id=$1";

async fn refused_read(c: &Case, fleet: &Uuid7, code: error_code::ErrorCode, why: &str) {
    let refused = c
        .creations
        .read(c.base.access(), fleet)
        .await
        .expect_err(why);
    assert_eq!(refused.code(), code, "{why}");
}

async fn corrupt_head(c: &Case, fleet: &Uuid7, review: Option<&str>, foreign_source: bool) {
    sqlx::query(CORRUPT_HEAD)
        .bind(fleet)
        .bind(mint())
        .bind(review)
        .bind(foreign_source)
        .execute(&mut *c.base.connection().await)
        .await
        .expect("an appended revision another writer got wrong");
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_saved_fleet_whose_stored_setup_is_unreadable_reads_as_a_stored_fault() {
    let c = Case::create().await;
    let (_, request) = c.request("stored-faults", Start::None).await;
    let fleet = c.save(&mint(), request).await.id;
    let initial: String =
        sqlx::query_scalar("SELECT initial_start::text FROM fleet.fleets WHERE id=$1")
            .bind(&fleet)
            .fetch_one(&mut *c.base.connection().await)
            .await
            .expect("the saved initial start");
    let set_initial = |value: String| {
        let c = &c;
        let fleet = &fleet;
        async move {
            sqlx::query("UPDATE fleet.fleets SET initial_start=$2::jsonb WHERE id=$1")
                .bind(fleet)
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
    corrupt_head(&c, &fleet, None, true).await;
    refused_read(
        &c,
        &fleet,
        error_code::INTERNAL_DB_QUERY,
        "a source candidate this daemon never minted",
    )
    .await;
    corrupt_head(&c, &fleet, Some(r#"{"subject":"unknown"}"#), false).await;
    refused_read(
        &c,
        &fleet,
        error_code::INTERNAL_DB_QUERY,
        "a frozen review of no known shape",
    )
    .await;
    c.cleanup().await;
}
