//! The model registry's row-decided outcomes.
//!
//! Everything here answers from a ROW, which is why none of it is provable at
//! router tier: a duplicate pair is a unique index refusing, an active entry is
//! a selection and an entry agreeing on `(secret_ref, model_id)`. Over an
//! unreachable pool each of these is the same 503, so the router suite proves
//! the layers in FRONT of them and this proves the outcomes themselves.
//!
//! The platform default's half — the reset's copy and the never-configured
//! view — is graded in `registry_default.rs`.

use std::sync::Arc;

use afd_billing::Posture;
use afd_core::clock::UnixMillis;
use afd_credential::provider::{Added, Providers, Removed, Retargeted, Selection};
use afd_crypto::entropy::Entropy;

use super::Fixture;

/// The instant every write here is stamped with.
pub(super) const NOW: UnixMillis = UnixMillis::from_millis(1_760_000_000_000);

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
/// `core.model_library` and `core.platform_provider_defaults` are both keyed
/// without a tenant column and this lane shares one database, so a fixed model
/// id is a ROW shared with every sibling test — the defect the activation
/// suite next door already paid for once.
pub(super) fn unique_model() -> String {
    format!("gpt-registry-{}", afd_db::test_util::mint_id())
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn removing_the_entry_the_tenant_runs_on_is_refused() {
    let fixture = Fixture::create().await;
    fixture
        .seed_with_shape(CREDENTIAL, BODY, Some(PROVIDER), Some(true))
        .await;
    let store = providers(&fixture);

    let running_on = unique_model();
    let spare = unique_model();
    let Added::Stored(active) = store
        .add_entry(&fixture.tenant, &running_on, CREDENTIAL, NOW)
        .await
        .expect("an entry over a stored credential is an outcome")
    else {
        panic!("the credential is stored, so the entry stores");
    };
    let Added::Stored(idle) = store
        .add_entry(&fixture.tenant, &spare, CREDENTIAL, NOW)
        .await
        .expect("a second model on the same credential is a second entry")
    else {
        panic!("a different model on the same credential is not a duplicate");
    };

    // The selection is what makes one of the two entries the active one, and it
    // matches by `(secret_ref, model_id)` — there is no `active` column to set.
    store
        .upsert(
            &fixture.tenant,
            &Selection {
                posture: Posture::SelfManaged,
                provider: PROVIDER.into(),
                model: running_on.clone().into_boxed_str(),
                context_cap_tokens: 128_000,
                secret_ref: Some(CREDENTIAL.into()),
            },
            NOW,
        )
        .await
        .expect("the selection writes");

    assert_eq!(
        store
            .remove_entry(&fixture.tenant, &active.id)
            .await
            .expect("removing the active entry is an outcome, not a failure"),
        Removed::Active,
        "deleting it would leave the selection naming a row that is gone"
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
        store
            .entry(&fixture.tenant, &idle.id)
            .await
            .expect("the read answers")
            .is_none(),
        "Done means gone, not merely permitted"
    );
    assert!(
        store
            .entry(&fixture.tenant, &active.id)
            .await
            .expect("the read answers")
            .is_some(),
        "the refused delete left its row where it was"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_same_model_on_the_same_credential_is_refused_as_a_duplicate() {
    let fixture = Fixture::create().await;
    fixture
        .seed_with_shape(CREDENTIAL, BODY, Some(PROVIDER), Some(true))
        .await;
    let store = providers(&fixture);

    let first = unique_model();
    let second = unique_model();
    let Added::Stored(stored) = store
        .add_entry(&fixture.tenant, &first, CREDENTIAL, NOW)
        .await
        .expect("the first entry stores")
    else {
        panic!("the credential is stored, so the entry stores");
    };

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
    let Added::Stored(moved) = store
        .add_entry(&fixture.tenant, &second, CREDENTIAL, NOW)
        .await
        .expect("a second model on the same credential stores")
    else {
        panic!("a different model on the same credential is not a duplicate");
    };
    assert_eq!(
        store
            .set_entry_model(&fixture.tenant, &moved.id, &first, NOW)
            .await
            .expect("a collision on retarget is an outcome"),
        Retargeted::Duplicate,
    );

    // And the control: retargeting onto a model this credential does NOT carry
    // is stored, so the refusal above is the collision and not the verb.
    let third = unique_model();
    let Retargeted::Stored(retargeted) = store
        .set_entry_model(&fixture.tenant, &moved.id, &third, NOW)
        .await
        .expect("an uncollided retarget is an outcome")
    else {
        panic!("nothing else carries that model, so it moves");
    };
    assert_eq!(&*retargeted.model_id, third.as_str());
    assert_eq!(retargeted.id, moved.id, "retargeting keeps the entry");
    assert_eq!(
        &*retargeted.secret_ref, CREDENTIAL,
        "and keeps its credential — the model moves, the binding does not"
    );

    assert_eq!(&*stored.model_id, first.as_str());
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
    let stranger = afd_core::id::Uuid7::parse(&stranger).expect("the minted id is UUIDv7");

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
            .set_entry_model(&fixture.tenant, &stranger, "claude-opus-5", NOW)
            .await
            .expect("an unknown id is an outcome"),
        Retargeted::NotFound,
    );

    fixture.cleanup().await;
}
