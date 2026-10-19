//! Two tenants, one table: every registry verb answers only for its own rows.
//!
//! `model.tenant_models` holds every tenant's entries, and each statement
//! over it carries the tenant in its predicate. An entry id is not a secret —
//! it travels in item routes — so a statement that lost the predicate would
//! hand one tenant's row to another the moment its id was guessed or leaked.
//! Each verb is driven here with the OTHER tenant's id.

use afd_core::id::Uuid7;
use afd_credential::provider::{Removed, Retargeted};

use super::Fixture;
use super::registry_page::{BODY, CREDENTIAL, add, providers, unique_model, unique_provider};

/// How many rows a page asks for when it wants the whole registry.
const WHOLE_PAGE: u32 = 50;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_registry_never_answers_for_another_tenants_entry() {
    let owner = Fixture::create().await;
    let other = Fixture::create().await;
    owner
        .seed_with_shape(CREDENTIAL, BODY, Some(&unique_provider()), Some(true))
        .await;
    let store = providers(&owner);
    let model = unique_model();
    let held = add(&store, &owner, &model).await;

    let page = store
        .registry_page(&other.tenant, WHOLE_PAGE, None)
        .await
        .expect("the other tenant's page reads");
    assert!(
        page.rows.iter().all(|row| row.entry.id != held.id),
        "another tenant's page never lists this entry"
    );
    assert_eq!(
        store
            .entry(&other.tenant, &held.id)
            .await
            .expect("the by-id read answers"),
        None
    );
    assert_eq!(
        store
            .set_entry_model(&other.tenant, &held.id, &unique_model())
            .await
            .expect("a foreign id is an outcome"),
        Retargeted::NotFound
    );
    assert_eq!(
        store
            .remove_entry(&other.tenant, &held.id)
            .await
            .expect("a foreign id is an outcome"),
        Removed::Done
    );

    let kept = still_held(&store, &owner.tenant, &held.id).await;
    assert_eq!(
        (&*kept.model_name, &*kept.secret_ref),
        (model.as_str(), CREDENTIAL),
        "the owner's entry is untouched by every foreign verb"
    );

    other.cleanup().await;
    owner.cleanup().await;
}

/// The owner's entry, which must still be there.
async fn still_held(
    store: &afd_credential::provider::Providers,
    tenant: &Uuid7,
    id: &Uuid7,
) -> afd_credential::provider::Entry {
    store
        .entry(tenant, id)
        .await
        .expect("the by-id read answers")
        .expect("the owner still holds its entry")
}
