//! A saved fleet whose stored setup cannot be read back is a stored fault,
//! never a missing fleet or a guess at what the row meant.

use super::super::mint;
use super::fixture::Case;
use af_core::{error_code, id::Uuid7};
use afd_setup::types::Start;

/// Appends a copy of the fleet's first revision, as the next one, with one
/// field another writer got wrong, and moves the head to it: `$3` as its
/// frozen review, `$4` no frozen review.
///
/// Copied from the first revision, so each fault is the only one the head
/// holds.
const CORRUPT_HEAD: &str = "WITH first AS ( \
         SELECT r.* FROM fleet.revisions r WHERE r.fleet_id=$1 AND r.revision=1), \
     appended AS ( \
         INSERT INTO fleet.revisions(id,fleet_id,revision,definition,canonical_digest, \
             policy_semantics,installed,runtime_model_ref,review_digest,review_context, \
             created_at,updated_at) \
         SELECT $2,fleet_id, \
             (SELECT max(revision)+1 FROM fleet.revisions WHERE fleet_id=$1), \
             definition,canonical_digest,policy_semantics,true,runtime_model_ref,review_digest, \
             CASE WHEN $4 THEN NULL ELSE COALESCE($3::jsonb,review_context) END, \
             now(),now() \
             FROM first RETURNING id) \
     UPDATE fleet.fleets SET head_revision_id=(SELECT id FROM appended) WHERE id=$1";

/// The one field a corrupted head holds wrong.
enum Fault<'a> {
    Review(&'a str),
    NoReview,
}

async fn refused_read(c: &Case, fleet: &Uuid7, code: error_code::ErrorCode, why: &str) {
    let refused = c
        .creations
        .read(c.base.access(), fleet)
        .await
        .expect_err(why);
    assert_eq!(refused.code(), code, "{why}");
}

async fn corrupt_head(c: &Case, fleet: &Uuid7, fault: &Fault<'_>) {
    sqlx::query(CORRUPT_HEAD)
        .bind(fleet)
        .bind(mint())
        .bind(match fault {
            Fault::Review(review) => Some(*review),
            Fault::NoReview => None,
        })
        .bind(matches!(fault, Fault::NoReview))
        .execute(&mut *c.base.connection().await)
        .await
        .expect("an appended revision another writer got wrong");
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
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
    let set_initial = |value: Option<String>| {
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
    set_initial(Some(r#"{"kind":"run_later"}"#.to_owned())).await;
    refused_read(
        &c,
        &fleet,
        error_code::INTERNAL_DB_QUERY,
        "an initial start of no known kind",
    )
    .await;
    set_initial(Some(initial.clone())).await;
    c.creations
        .read(c.base.access(), &fleet)
        .await
        .expect("the restored row reads");
    set_initial(None).await;
    refused_read(
        &c,
        &fleet,
        error_code::INTERNAL_DB_QUERY,
        "no initial start",
    )
    .await;
    set_initial(Some(initial)).await;
    for (fault, why) in [
        (
            Fault::Review(r#"{"subject":"unknown"}"#),
            "a frozen review of no known shape",
        ),
        (Fault::NoReview, "no frozen review"),
    ] {
        corrupt_head(&c, &fleet, &fault).await;
        refused_read(&c, &fleet, error_code::INTERNAL_DB_QUERY, why).await;
    }
    c.cleanup().await;
}
