//! The registry PAGE — the read the Models table is drawn from.
//!
//! Its siblings next door grade the write verbs, each of which answers from one
//! row. This grades the composition instead: six independent reads folded into
//! one page, where every interesting property is a relationship BETWEEN them.
//! Which row is active is a selection and an entry agreeing; what a row costs is
//! a catalogue row reached through the credential's provider; whether a later
//! page exists is a row fetched and deliberately not served. None of those is
//! visible from any single read, and none is reachable over an unreachable pool,
//! which is why they are proven here rather than at router tier.
//!
//! # Every name is unique per test, and that is not incidental
//!
//! `core.model_library` and `core.platform_provider_defaults` carry no tenant
//! column, and this lane shares one database across every test in it. A fixed
//! model id or provider name is therefore a row shared with every sibling —
//! the defect the activation suite already paid for once. Each test below mints
//! its own.

use std::sync::Arc;

use afd_billing::Posture;
use afd_core::clock::UnixMillis;
use afd_credential::provider::{Added, Providers, Selection};
use afd_crypto::entropy::Entropy;

use super::Fixture;

/// The instant every write here is stamped with.
pub(super) const NOW: UnixMillis = UnixMillis::from_millis(1_760_000_000_000);

/// The vault key name the entries below hang off.
pub(super) const CREDENTIAL: &str = "a-paged-provider-key";

/// A credential body the page never opens — [`Directory`](afd_vault::Directory)
/// cannot decrypt, which is the guarantee the page's own header claims.
pub(super) const BODY: &[u8] = br#"{"provider":"openai","api_key":"sk-live"}"#;

/// The context ceiling the seeded catalogue rows carry.
pub(super) const CAP: i32 = 128_000;

/// The store under test, over the fixture's pool and key.
pub(super) fn providers(fixture: &Fixture) -> Providers {
    Providers::new(
        fixture.database.clone(),
        Arc::clone(&fixture.kek),
        Entropy::new(),
    )
}

/// A model id this test alone names.
pub(super) fn unique_model() -> String {
    format!("gpt-page-{}", afd_db::test_util::mint_id())
}

/// A provider name this test alone publishes under.
pub(super) fn unique_provider() -> String {
    format!("page-fixture-{}", afd_db::test_util::mint_id())
}

/// Stamps `meta_kind` on a seeded credential, which `seed_with_shape` leaves
/// NULL.
///
/// Without it the descriptor degrades: `labelled` answers `None` for a row
/// carrying no spelling, and the page then renders an opaque secret with no
/// provider and no base URL, whatever the other `meta_*` columns hold. That
/// degrade is real behaviour and the dangling-reference test below leans on the
/// same path — this is how a row that IS labelled gets to prove the other half.
pub(super) async fn label_kind(fixture: &Fixture, name: &str, kind: &str) {
    let mut connection = fixture.database.acquire().await.expect("an API connection");
    sqlx::query(
        "UPDATE vault.secrets SET meta_kind = $3 \
         WHERE workspace_id = $1::uuid AND key_name = $2",
    )
    .bind(fixture.workspace.as_str())
    .bind(name)
    .bind(kind)
    .execute(&mut *connection)
    .await
    .expect("the kind column seeds");
}

/// Registers `model` on the fixture credential and answers the stored entry.
pub(super) async fn add(
    store: &Providers,
    fixture: &Fixture,
    model: &str,
) -> afd_credential::provider::Entry {
    let Added::Stored(entry) = store
        .add_entry(&fixture.tenant, model, CREDENTIAL, NOW)
        .await
        .expect("an entry over a stored credential is an outcome")
    else {
        panic!("the credential is stored, so the entry stores");
    };
    entry
}

/// A row carries the vault's labels, the catalogue's price, and the selection's
/// verdict on whether the tenant is running on it.
///
/// The three come from three different reads, and the row is the only place
/// they meet. `active` is the sharpest of them: there is no `active` column, so
/// a page that got it right did so by matching `(secret_ref, model_id)` against
/// the selection — and the second entry proves it discriminates rather than
/// flagging everything it finds.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_row_carries_its_credential_its_rate_and_the_selection_verdict() {
    let fixture = Fixture::create().await;
    let provider = unique_provider();
    fixture
        .seed_with_shape(CREDENTIAL, BODY, Some(&provider), Some(true))
        .await;
    label_kind(&fixture, CREDENTIAL, afd_vault::Kind::ProviderKey.as_str()).await;
    let store = providers(&fixture);

    let running_on = unique_model();
    let spare = unique_model();
    fixture.seed_catalogue(&provider, &running_on, CAP).await;
    fixture.seed_catalogue(&provider, &spare, CAP).await;
    let active = add(&store, &fixture, &running_on).await;
    let idle = add(&store, &fixture, &spare).await;

    store
        .upsert(
            &fixture.tenant,
            &Selection {
                posture: Posture::SelfManaged,
                provider: provider.clone().into_boxed_str(),
                model: running_on.clone().into_boxed_str(),
                context_cap_tokens: 128_000,
                secret_ref: Some(CREDENTIAL.into()),
            },
            NOW,
        )
        .await
        .expect("the selection writes");

    let page = store
        .registry_page(&fixture.tenant, 25, None)
        .await
        .expect("the page reads");

    assert_eq!(page.rows.len(), 2, "both entries are this tenant's");
    let flagged = page
        .rows
        .iter()
        .find(|row| row.entry.id == active.id)
        .expect("the active entry is on the page");
    let unflagged = page
        .rows
        .iter()
        .find(|row| row.entry.id == idle.id)
        .expect("the idle entry is on the page");

    assert!(flagged.active, "the selection names this entry's pair");
    assert!(
        !unflagged.active,
        "a second entry on the SAME credential is not what the tenant runs on"
    );

    let held = flagged
        .credential
        .as_ref()
        .expect("the vault describes a credential it holds");
    assert_eq!(held.kind(), afd_vault::Kind::ProviderKey);
    assert_eq!(held.provider(), Some(provider.as_str()));
    assert!(held.has_key, "the seeded projection says a key is stored");

    let rate = flagged
        .rate
        .as_ref()
        .expect("a catalogue row exists for this provider and model");
    assert_eq!(rate.context_cap_tokens, u32::try_from(CAP).unwrap_or(0));

    fixture.cleanup().await;
}

/// An entry naming a credential the vault no longer holds still lists, degraded
/// rather than absent — and it does not take the rest of the page with it.
///
/// This is the dangling-reference case: a page of twenty models must not fail
/// over one credential deleted out of band, so the row comes back with no
/// descriptor and no rate, and the sibling row beside it is untouched.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_entry_whose_credential_is_gone_lists_degraded_beside_an_intact_row() {
    let fixture = Fixture::create().await;
    let provider = unique_provider();
    fixture
        .seed_with_shape(CREDENTIAL, BODY, Some(&provider), Some(true))
        .await;
    let store = providers(&fixture);

    let orphaned_model = unique_model();
    fixture
        .seed_catalogue(&provider, &orphaned_model, CAP)
        .await;
    let orphaned = add(&store, &fixture, &orphaned_model).await;

    // A SECOND credential, kept, so the page has an intact row to prove the
    // degradation is per-row rather than a page-wide give-up.
    let kept_name = "a-kept-provider-key";
    let kept_model = unique_model();
    fixture
        .seed_with_shape(kept_name, BODY, Some(&provider), Some(true))
        .await;
    fixture.seed_catalogue(&provider, &kept_model, CAP).await;
    let kept = {
        let Added::Stored(entry) = store
            .add_entry(&fixture.tenant, &kept_model, kept_name, NOW)
            .await
            .expect("the second entry is an outcome")
        else {
            panic!("the second credential is stored, so the entry stores");
        };
        entry
    };

    // Out of band: the registry keeps its row, the vault loses its own.
    let mut connection = fixture.database.acquire().await.expect("an API connection");
    sqlx::query("DELETE FROM vault.secrets WHERE workspace_id = $1::uuid AND key_name = $2")
        .bind(fixture.workspace.as_str())
        .bind(CREDENTIAL)
        .execute(&mut *connection)
        .await
        .expect("the credential is deleted out from under its entry");
    drop(connection);

    let page = store
        .registry_page(&fixture.tenant, 25, None)
        .await
        .expect("one dangling reference does not fail the page");

    assert_eq!(page.rows.len(), 2, "the orphaned row still lists");
    let degraded = page
        .rows
        .iter()
        .find(|row| row.entry.id == orphaned.id)
        .expect("the orphaned entry is on the page");
    assert!(
        degraded.credential.is_none(),
        "the vault cannot describe what it no longer holds"
    );
    assert!(
        degraded.rate.is_none(),
        "a rate is reached through the credential's provider, which is gone"
    );

    let intact = page
        .rows
        .iter()
        .find(|row| row.entry.id == kept.id)
        .expect("the intact entry is on the page");
    assert!(
        intact.credential.is_some(),
        "its neighbour's deletion is not its own"
    );

    fixture.cleanup().await;
}
