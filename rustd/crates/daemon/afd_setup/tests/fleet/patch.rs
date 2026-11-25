//! A fleet's model is a field a person patches, checked as a create checks it.
//!
//! A change that is taken, and the next run it reaches, is proven where a run
//! can be delivered: `afd_fleet`'s run-gate suite.

use af_core::error_code;
use af_core::id::Uuid7;
use af_wire::policy::HttpEgress;
use afd_api_wire::fleet::{PatchFleetRequest, PatchStatus};

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

// Value: protects=a patch that names no field is refused before any row is
// read; fails_when=an empty patch bumps the fleet's tag; seam=`Creations::patch`
// over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_empty_patch_is_refused() {
    let f = Fixture::create().await;
    let fleet = created(&f).await;

    let refused = f
        .creations
        .patch(f.access(), &fleet.id, Some(&fleet.etag), &valid(patch()))
        .await
        .expect_err("a patch names a field");

    assert_eq!(refused.code(), error_code::INVALID_REQUEST);
    let read = f
        .creations
        .read(f.access(), &fleet.id)
        .await
        .expect("the fleet reads");
    assert_eq!(read.etag, fleet.etag);
    f.cleanup().await;
}

// Value: protects=an archived fleet takes no further change, and a rename onto
// a name another fleet holds is refused; fails_when=either patch writes the
// row; seam=`Creations::patch` over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_archived_fleet_and_a_taken_name_change_nothing() {
    let f = Fixture::create().await;
    let (taken, fleet) = (created(&f).await, created(&f).await);
    let rename = |name: &str| PatchFleetRequest {
        name: Some(name.to_owned()),
        ..patch()
    };

    let clash = f
        .creations
        .patch(
            f.access(),
            &fleet.id,
            Some(&fleet.etag),
            &valid(rename(&taken.name)),
        )
        .await
        .expect_err("another fleet holds the name");
    let archive = PatchFleetRequest {
        status: Some(PatchStatus::Archived),
        ..patch()
    };
    let archived = f
        .creations
        .patch(f.access(), &fleet.id, Some(&fleet.etag), &valid(archive))
        .await
        .expect("the owner archives the fleet");
    let late = f
        .creations
        .patch(
            f.access(),
            &fleet.id,
            Some(&archived.etag),
            &valid(rename("support-triage")),
        )
        .await
        .expect_err("an archived fleet is final");

    assert_eq!(clash.code(), error_code::AGENTSFLEET_NAME_EXISTS);
    assert_eq!(late.code(), error_code::AGENTSFLEET_ARCHIVED);
    assert_eq!(
        archived.name, fleet.name,
        "the clashing rename wrote nothing"
    );
    f.cleanup().await;
}

// Value: protects=a person's network toggle opens the fleet and closes it
// again; fails_when=the closing patch leaves the fleet public;
// seam=`Creations::patch` over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_network_toggle_opens_and_closes_the_fleet() {
    let f = Fixture::create().await;
    let fleet = created(&f).await;
    let toggle = |egress| PatchFleetRequest {
        http_egress: Some(egress),
        ..patch()
    };

    let opened = f
        .creations
        .patch(
            f.access(),
            &fleet.id,
            Some(&fleet.etag),
            &valid(toggle(HttpEgress::Public)),
        )
        .await
        .expect("the owner opens the fleet");
    let closed = f
        .creations
        .patch(
            f.access(),
            &fleet.id,
            Some(&opened.etag),
            &valid(toggle(HttpEgress::Restricted)),
        )
        .await
        .expect("the owner closes it again");

    assert_eq!(opened.http_egress, HttpEgress::Public);
    assert_eq!(closed.http_egress, HttpEgress::Restricted);
    f.cleanup().await;
}
