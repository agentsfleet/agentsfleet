//! A fleet's model is a field a person patches, checked as a create checks it.

use af_core::error_code;
use af_core::id::Uuid7;
use afd_api_wire::fleet::PatchFleetRequest;

use super::fixture::{Fixture, patch, valid};
use super::mint;

/// The stored kind of the receipt a model change writes to the fleet.
const MODEL_CHANGED: &str = "model_changed";

/// A fleet the owner created on the fixture's model.
async fn created(f: &Fixture) -> afd_setup::types::SavedFleet {
    f.creations
        .create(
            f.access(),
            &valid(f.request(&mint())),
            super::fixture::now(),
        )
        .await
        .expect("the owner creates a fleet")
        .fleet
}

/// How many model-change receipts `fleet` holds.
async fn model_changes(f: &Fixture, fleet: &Uuid7) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM thread.turns WHERE thread_id = $1 AND kind = $2")
        .bind(fleet)
        .bind(MODEL_CHANGED)
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("the fleet's receipts")
}

// Value: protects=moving a fleet to another eligible model is one patch that
// stores the model and writes one receipt; fails_when=the patch skips the
// receipt, writes two, or leaves the old model; seam=`Creations::patch` over
// live Postgres; no_claim=the next run's provider, which the run path reads
// from the same column.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_model_change_is_one_patch() {
    let f = Fixture::create().await;
    let fleet = created(&f).await;
    let change = PatchFleetRequest {
        model_ref: Some(f.other_model.to_string()),
        ..patch()
    };

    let patched = f
        .creations
        .patch(f.access(), &fleet.id, Some(&fleet.etag), &valid(change))
        .await
        .expect("an eligible model is accepted");

    assert_eq!(patched.model_ref, f.other_model.to_string());
    assert_ne!(patched.etag, fleet.etag, "the patch moves the tag");
    assert_eq!(model_changes(&f, &fleet.id).await, 1);
    f.cleanup().await;
}

// Value: protects=a model the workspace cannot run is refused as a create
// refuses it, and the fleet keeps its model; fails_when=the patch stores an
// unregistered reference; seam=`Creations::patch` over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_ineligible_model_ref_changes_nothing() {
    let f = Fixture::create().await;
    let fleet = created(&f).await;
    let change = PatchFleetRequest {
        model_ref: Some(mint().to_string()),
        ..patch()
    };

    let refused = f
        .creations
        .patch(f.access(), &fleet.id, Some(&fleet.etag), &valid(change))
        .await
        .expect_err("no registered model answers to the reference");

    assert_eq!(refused.code(), error_code::SETUP_MODEL);
    let read = f
        .creations
        .read(f.access(), &fleet.id)
        .await
        .expect("the fleet reads");
    assert_eq!(read.model_ref, fleet.model_ref);
    assert_eq!(read.etag, fleet.etag);
    assert_eq!(model_changes(&f, &fleet.id).await, 0);
    f.cleanup().await;
}
