//! The selection is a flag, proved against real rows.
//!
//! `model.tenant_models.selected` marks at most one of a tenant's rows, held
//! to one by the deferrable `ex_tenant_models_one_selected` exclusion. Provider
//! set writes the entry and selects it, moving the flag in one UPDATE; reset
//! clears it and keeps the row.

use af_core::id::Uuid7;
use afd_credential::provider::{Activation, OwnModel, Providers, Retargeted, Selection};

use super::activation::{catalogue, unique_model, written};
use super::registry::providers;
use super::{Fixture, NOW};

/// The vault key name the tenant activates.
const CREDENTIAL: &str = "a-pointed-provider-key";

/// A credential body that decrypts and vets as a named provider key.
const BODY: &[u8] = br#"{"provider":"openai","api_key":"sk-live"}"#;

/// The provider the body names, and the catalogue row is published under.
const PROVIDER: &str = "openai";

/// The exclusion that keeps one selected row per tenant.
const ONE_SELECTED: &str = "ex_tenant_models_one_selected";

/// Marks one more row selected without clearing the tenant's other one.
const SELECT_ALONGSIDE: &str = "UPDATE model.tenant_models SET selected = true WHERE id = $1";

/// Activates `model` on [`CREDENTIAL`] and answers the entry it selected.
async fn applied(store: &Providers, fixture: &Fixture, model: &str) -> OwnModel {
    let Activation::Applied(own) = store
        .activate(&fixture.tenant, fixture.key(CREDENTIAL), Some(model), NOW)
        .await
        .expect("a catalogued activation is an outcome")
    else {
        panic!("the model is catalogued, so the activation applies");
    };
    own
}

/// The entry the tenant selected, read back.
async fn pointed_at(store: &Providers, fixture: &Fixture) -> OwnModel {
    let Selection::SelfManaged(own) = store
        .selection(&fixture.tenant)
        .await
        .expect("the selection reads")
    else {
        panic!("an activated tenant is self-managed");
    };
    own
}

/// Selects `entry` beside the tenant's selected row, answering the refusal.
async fn refused_second_selection(fixture: &Fixture, entry: &Uuid7) -> sqlx::Error {
    let mut connection = fixture.database.acquire().await.expect("an API connection");
    sqlx::query(SELECT_ALONGSIDE)
        .bind(entry)
        .execute(&mut *connection)
        .await
        .expect_err("a second selected row is refused")
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_tenant_selection_points_at_model() {
    let fixture = Fixture::create().await;
    fixture
        .seed_with_shape(CREDENTIAL, BODY, Some(PROVIDER), Some(true))
        .await;
    let (model, other_model) = (unique_model(), unique_model());
    catalogue(&fixture, PROVIDER, &model, 200_000).await;
    catalogue(&fixture, PROVIDER, &other_model, 64_000).await;
    let store = providers(&fixture);

    // Provider set: one entry written, and it is the selected one.
    let written_back = applied(&store, &fixture, &model).await;
    assert_eq!(written(&fixture).await, (1, 1));
    let read = pointed_at(&store, &fixture).await;
    assert_eq!(read, written_back, "the read agrees with the write's echo");
    let entry = store
        .entry(&fixture.tenant, &read.tenant_model_id)
        .await
        .expect("the entry reads")
        .expect("the selection names a real entry");
    assert_eq!(&*entry.model_name, model.as_str());
    assert_eq!(read.context_cap_tokens, 200_000, "the catalogue's ceiling");

    // Reset: the flag clears, the entry is untouched.
    store
        .reset(&fixture.tenant)
        .await
        .expect("the reset writes");
    assert_eq!(
        store
            .selection(&fixture.tenant)
            .await
            .expect("the selection reads"),
        Selection::Platform
    );
    assert_eq!(written(&fixture).await, (0, 1), "reset deletes nothing");

    // Moving the selection between two rows is one UPDATE whatever order it
    // meets them in: the second activation selects the new entry and clears
    // the first, and one row stays selected.
    applied(&store, &fixture, &model).await;
    let moved = applied(&store, &fixture, &other_model).await;
    assert_eq!(written(&fixture).await, (1, 2));
    assert_eq!(pointed_at(&store, &fixture).await, moved);

    // A raw write selecting the first row beside it is refused.
    let refused = refused_second_selection(&fixture, &entry.id).await;
    assert_eq!(
        refused
            .as_database_error()
            .and_then(|failure| failure.constraint()),
        Some(ONE_SELECTED),
        "refused by the one-selected exclusion: {refused}"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn retargeting_the_active_entry_moves_what_the_tenant_runs_on() {
    // The selection is the entry itself, not a copy of its pair, so changing
    // the entry's model changes the next run's model with it.
    let fixture = Fixture::create().await;
    fixture
        .seed_with_shape(CREDENTIAL, BODY, Some(PROVIDER), Some(true))
        .await;
    let (before, after) = (unique_model(), unique_model());
    catalogue(&fixture, PROVIDER, &before, 200_000).await;
    catalogue(&fixture, PROVIDER, &after, 64_000).await;
    let store = providers(&fixture);
    let activated = applied(&store, &fixture, &before).await;

    let Retargeted::Stored(_) = store
        .set_entry_model(&fixture.tenant, &activated.tenant_model_id, &after)
        .await
        .expect("a retarget is an outcome")
    else {
        panic!("the entry is the tenant's own, so it moves");
    };

    let read = pointed_at(&store, &fixture).await;
    assert_eq!(read.tenant_model_id, activated.tenant_model_id);
    assert_eq!(&*read.model, after.as_str());
    assert_eq!(read.context_cap_tokens, 64_000, "the new model's ceiling");

    fixture.cleanup().await;
}
