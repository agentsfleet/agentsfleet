//! The selection is a pointer: Dimension 4.4 against real rows.
//!
//! `model.tenant_selections.tenant_model_id` names one `model.tenant_models`
//! row through a composite foreign key that carries `tenant_id` too, so the
//! database itself refuses a pointer at another tenant's model. Provider set
//! writes the entry and points at it; reset stores NULL and keeps the row.

use afd_core::id::Uuid7;
use afd_credential::provider::{Activation, OwnModel, Providers, Retargeted, Selection};

use super::activation::{catalogue, unique_model, written};
use super::registry::providers;
use super::{Fixture, NOW, POINT_SELECTION};

/// The vault key name the tenant activates.
const CREDENTIAL: &str = "a-pointed-provider-key";

/// A credential body that decrypts and vets as a named provider key.
const BODY: &[u8] = br#"{"provider":"openai","api_key":"sk-live"}"#;

/// The provider the body names, and the catalogue row is published under.
const PROVIDER: &str = "openai";

/// The foreign key that keeps a pointer inside its own tenant.
const FK_TENANT_MODEL: &str = "fk_tenant_selections_tenant_model";

/// Activates `model` on [`CREDENTIAL`] and answers the entry it points at.
async fn applied(store: &Providers, fixture: &Fixture, model: &str) -> OwnModel {
    let Activation::Applied(own) = store
        .activate(&fixture.tenant, CREDENTIAL, Some(model), NOW)
        .await
        .expect("a catalogued activation is an outcome")
    else {
        panic!("the model is catalogued, so the activation applies");
    };
    own
}

/// The entry the tenant's selection points at, read back.
async fn pointed_at(store: &Providers, fixture: &Fixture) -> OwnModel {
    let Some(Selection::SelfManaged(own)) = store
        .selection(&fixture.tenant)
        .await
        .expect("the selection reads")
    else {
        panic!("an activated tenant is self-managed");
    };
    own
}

/// Points `tenant`'s selection at `entry` as a raw row, answering the refusal.
async fn refused_pointer(fixture: &Fixture, tenant: &Uuid7, entry: &Uuid7) -> sqlx::Error {
    let mut connection = fixture.database.acquire().await.expect("an API connection");
    sqlx::query(POINT_SELECTION)
        .bind(tenant.as_str())
        .bind(entry.as_str())
        .execute(&mut *connection)
        .await
        .expect_err("a pointer at another tenant's model is refused")
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_tenant_selection_points_at_model() {
    let fixture = Fixture::create().await;
    fixture
        .seed_with_shape(CREDENTIAL, BODY, Some(PROVIDER), Some(true))
        .await;
    let model = unique_model();
    catalogue(&fixture, PROVIDER, &model, 200_000).await;
    let store = providers(&fixture);

    // Provider set: one entry written, and the selection points at it.
    let written_back = applied(&store, &fixture, &model).await;
    assert_eq!(written(&fixture).await, (1, 1));
    let read = pointed_at(&store, &fixture).await;
    assert_eq!(read, written_back, "the read agrees with the write's echo");
    let entry = store
        .entry(&fixture.tenant, &read.tenant_model_id)
        .await
        .expect("the entry reads")
        .expect("the pointer names a real entry");
    assert_eq!(&*entry.model_name, model.as_str());
    assert_eq!(read.context_cap_tokens, 200_000, "the catalogue's ceiling");

    // Reset: the row stays, the pointer goes NULL, the entry is untouched.
    store
        .reset(&fixture.tenant)
        .await
        .expect("the reset writes");
    assert_eq!(
        store
            .selection(&fixture.tenant)
            .await
            .expect("the selection reads"),
        Some(Selection::Platform)
    );
    assert_eq!(written(&fixture).await, (1, 1), "reset deletes nothing");

    // Another tenant's model: the composite foreign key refuses the pointer.
    let other = Fixture::create().await;
    let refused = refused_pointer(&fixture, &other.tenant, &entry.id).await;
    assert!(
        afd_db::constraint::violates_foreign_key(&refused, FK_TENANT_MODEL),
        "refused by the tenant-carrying foreign key: {refused}"
    );

    other.cleanup().await;
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn retargeting_the_active_entry_moves_what_the_tenant_runs_on() {
    // The selection points at the entry, not at a copy of its pair, so
    // changing the entry's model changes the next run's model with it.
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
