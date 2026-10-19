//! The model registry's row-decided outcomes.
//!
//! Everything here answers from a ROW, which is why none of it is provable at
//! router tier: a duplicate pair is a unique index refusing, an active entry is
//! the one the selection's pointer names. Over an
//! unreachable pool each of these is the same 503, so the router suite proves
//! the layers in FRONT of them and this proves the outcomes themselves.
//!
//! The platform default's half — the reset and the never-configured view — is
//! graded in `registry_default.rs`.

use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_credential::provider::{
    Activation, Added, Entry, OwnModel, Providers, Removed, Retargeted,
};
use afd_crypto::entropy::Entropy;

use super::{Fixture, NOW};

/// The vault key name the registry entries below hang off.
const CREDENTIAL: &str = "a-provider-key";

/// A credential body the entry writes never open — `add_entry` locks the row
/// for reference and reads no plaintext, which is the point of the lock.
const BODY: &[u8] = br#"{"provider":"openai","api_key":"sk-live"}"#;

/// The provider the registry entries are published under.
const PROVIDER: &str = "openai";

/// The store under test, over the fixture's pool and key.
pub(super) fn providers(fixture: &Fixture) -> Providers {
    Providers::new(
        fixture.database.clone(),
        Arc::clone(&fixture.kek),
        Entropy::new(),
    )
}

/// A model id this test alone names.
///
/// `model.platform_models` and `model.platform_defaults` are both keyed
/// without a tenant column and this lane shares one database, so a fixed model
/// id is a ROW shared with every sibling test — the defect the activation
/// suite next door already paid for once.
pub(super) fn unique_model() -> String {
    format!("gpt-registry-{}", afd_db::test_util::mint_id())
}

/// Stores [`CREDENTIAL`] for the fixture tenant and answers the store over it.
async fn seeded(fixture: &Fixture) -> Providers {
    fixture
        .seed_with_shape(CREDENTIAL, BODY, Some(PROVIDER), Some(true))
        .await;
    providers(fixture)
}

/// Registers `model` on [`CREDENTIAL`] and answers the stored entry.
async fn stored(store: &Providers, fixture: &Fixture, model: &str) -> Entry {
    let Added::Stored(entry) = store
        .add_entry(&fixture.tenant, model, CREDENTIAL, NOW)
        .await
        .expect("an entry over a stored credential is an outcome")
    else {
        panic!("the credential is stored and the model is new, so the entry stores");
    };
    entry
}

/// Whether `tenant` still holds the entry `id`.
async fn holds(store: &Providers, fixture: &Fixture, id: &Uuid7) -> bool {
    store
        .entry(&fixture.tenant, id)
        .await
        .expect("the read answers")
        .is_some()
}

/// Activates `model` on [`CREDENTIAL`], which points the selection at its entry.
async fn activated(store: &Providers, fixture: &Fixture, model: &str) -> OwnModel {
    fixture.seed_catalogue(PROVIDER, model, 128_000).await;
    let Activation::Applied(chosen) = store
        .activate(&fixture.tenant, CREDENTIAL, Some(model), NOW)
        .await
        .expect("a catalogued activation is an outcome")
    else {
        panic!("the model is catalogued, so the activation applies");
    };
    chosen
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn removing_the_entry_the_tenant_runs_on_is_refused() {
    let fixture = Fixture::create().await;
    let store = seeded(&fixture).await;

    let running_on = unique_model();
    let active = stored(&store, &fixture, &running_on).await;
    let idle = stored(&store, &fixture, &unique_model()).await;

    // The selection is what makes one of the two entries the active one: its
    // pointer names that row, and there is no `active` column to set.
    // Activating the same pair finds the entry already there and points at it.
    let chosen = activated(&store, &fixture, &running_on).await;
    assert_eq!(
        chosen.tenant_model_id, active.id,
        "it reuses the existing entry"
    );

    assert_eq!(
        store
            .remove_entry(&fixture.tenant, &active.id)
            .await
            .expect("removing the active entry is an outcome, not a failure"),
        Removed::Active,
        "deleting it would silently point the tenant back at the platform default"
    );

    // The other half of the discrimination, and the reason the first assertion
    // is not just "delete always refuses": the same verb, the same tenant, the
    // same credential, and an entry nothing is running on comes out.
    assert_eq!(
        store
            .remove_entry(&fixture.tenant, &idle.id)
            .await
            .expect("removing an idle entry is an outcome"),
        Removed::Done,
    );
    assert!(
        !holds(&store, &fixture, &idle.id).await,
        "Done means gone, not merely permitted"
    );
    assert!(
        holds(&store, &fixture, &active.id).await,
        "the refused delete left its row where it was"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_same_model_on_the_same_credential_is_refused_as_a_duplicate() {
    let fixture = Fixture::create().await;
    let store = seeded(&fixture).await;

    let first = unique_model();
    let original = stored(&store, &fixture, &first).await;

    assert_eq!(
        store
            .add_entry(&fixture.tenant, &first, CREDENTIAL, NOW)
            .await
            .expect("a repeat is an outcome, not a unique-violation error"),
        Added::Duplicate,
        "the domain key is (tenant, model, credential) and this repeats it"
    );

    // The same collision reached through the OTHER verb. `set_entry_model`
    // cannot express it with `ON CONFLICT` — the row it would update is not the
    // row it is addressing — so it reads a unique violation back instead, and
    // that translation is what this pins.
    let moved = stored(&store, &fixture, &unique_model()).await;
    assert_eq!(
        store
            .set_entry_model(&fixture.tenant, &moved.id, &first)
            .await
            .expect("a collision on retarget is an outcome"),
        Retargeted::Duplicate,
    );

    // And the control: retargeting onto a model this credential does NOT carry
    // is stored, so the refusal above is the collision and not the verb.
    let third = unique_model();
    let Retargeted::Stored(retargeted) = store
        .set_entry_model(&fixture.tenant, &moved.id, &third)
        .await
        .expect("an uncollided retarget is an outcome")
    else {
        panic!("nothing else carries that model, so it moves");
    };
    assert_eq!(&*retargeted.model_name, third.as_str());
    assert_eq!(retargeted.id, moved.id, "retargeting keeps the entry");
    assert_eq!(
        &*retargeted.secret_ref, CREDENTIAL,
        "and keeps its credential — the model moves, the binding does not"
    );

    assert_eq!(&*original.model_name, first.as_str());
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_id_that_resolves_to_nothing_is_answered_by_each_write_verb_in_its_own_way() {
    let fixture = Fixture::create().await;
    let store = providers(&fixture);
    // Well-formed and minted here, so it belongs to no tenant at all — the
    // parse is not what refuses this, which is the whole point of asking it
    // against a live table rather than at router tier.
    let stranger = afd_db::test_util::mint_id();
    let stranger = Uuid7::parse(&stranger).expect("the minted id is UUIDv7");

    // The delete is IDEMPOTENT: a caller retrying one it never saw the answer
    // to wants the row gone, and it is. Reporting "missing" would invite a
    // repair there is nothing to repair.
    assert_eq!(
        store
            .remove_entry(&fixture.tenant, &stranger)
            .await
            .expect("an unknown id is an outcome"),
        Removed::Done,
    );

    // The change verb is NOT idempotent, and the asymmetry is deliberate: it
    // was asked to point a row at a model, and there is no row to point.
    assert_eq!(
        store
            .set_entry_model(&fixture.tenant, &stranger, "claude-opus-5")
            .await
            .expect("an unknown id is an outcome"),
        Retargeted::NotFound,
    );

    fixture.cleanup().await;
}

/// A tenant with no workspace holds no credential to register a model
/// against, and is told that rather than "credential missing": the two
/// remedies differ, and the lock that found no credential cannot tell them
/// apart on its own.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_entry_for_a_tenant_with_no_workspace_names_the_missing_workspace() {
    let fixture = Fixture::create().await;
    let store = seeded(&fixture).await;
    let orphan = fixture.seed_tenant_without_workspace().await;

    let outcome = store
        .add_entry(&orphan, &unique_model(), CREDENTIAL, NOW)
        .await
        .expect("a missing workspace is an outcome, not a fault");

    assert!(matches!(outcome, Added::NoWorkspace), "{outcome:?}");
    fixture.cleanup().await;
}

/// A retarget the database refuses for any reason but the domain key is the
/// store's failure, never a duplicate: only that one constraint is the
/// caller's to fix.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_retarget_the_database_refuses_off_the_domain_key_is_a_query_failure() {
    let fixture = Fixture::create().await;
    let store = seeded(&fixture).await;
    let entry = stored(&store, &fixture, &unique_model()).await;

    // Postgres refuses a NUL byte in text, so the update itself fails.
    let refused = store
        .set_entry_model(&fixture.tenant, &entry.id, "nul\0model")
        .await
        .expect_err("the statement is refused");

    assert_eq!(refused.code(), afd_core::error_code::INTERNAL_DB_QUERY);
    assert!(holds(&store, &fixture, &entry.id).await, "the entry stands");
    fixture.cleanup().await;
}

/// An envelope sealed under another key is a failure the inspection
/// preserves, never a credential classified from bytes that did not open.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_inspection_of_an_envelope_that_will_not_open_fails() {
    let fixture = Fixture::create().await;
    let foreign = afd_crypto::secret::Kek::from_bytes([0x24; 32]);
    let envelope = afd_crypto::envelope::Sealer::new()
        .seal(
            &foreign,
            &afd_crypto::aad::Aad::new(fixture.workspace.as_str(), CREDENTIAL),
            BODY,
        )
        .expect("the foreign envelope seals");
    fixture.insert(CREDENTIAL, &envelope).await;
    let mut connection = fixture.database.acquire().await.expect("an API connection");

    let refused = fixture
        .vault
        .inspect_on(
            &mut connection,
            &fixture.workspace,
            &[CREDENTIAL.to_owned()],
            &afd_credential::secrets::Registry::SHIPPED,
        )
        .await
        .expect_err("the envelope does not open under this vault's key");

    assert_eq!(
        refused.code(),
        afd_core::error_code::INTERNAL_OPERATION_FAILED
    );
    drop(connection);
    fixture.cleanup().await;
}
