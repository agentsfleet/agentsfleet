//! The platform default as the registry reads it: the reset's copy of it into
//! a tenant's explicit platform row, and the view a tenant that never
//! configured a provider is composed from.
//!
//! Split from `registry.rs` by concern: those are the entry verbs, decided by
//! a tenant's own rows; these are decided by a deployment-wide row no tenant
//! owns.
//!
//! # What stays ungraded, and why it is not an oversight
//!
//! The reset's `UZ-PROVIDER-009` refusal fires when NO platform default is
//! active, and `core.platform_provider_defaults` carries no tenant column —
//! `active = true` is a fact about the whole deployment. This lane shares one
//! database across every test in it, so a case asserting that table is empty
//! would be asserting something a sibling test can falsify by seeding its own
//! default. What is graded below is the half that IS per-tenant: that an active
//! default is read and copied verbatim into the tenant's explicit platform row.

use afd_billing::Posture;
use afd_credential::provider::Selection;

use super::Fixture;
use super::registry::{NOW, providers, unique_model};

/// A provider name for the platform default this suite publishes.
///
/// `core.platform_provider_defaults` is keyed BY provider, so seeding under a
/// name a sibling test also uses would rewrite that test's row instead of
/// adding one. Unique per run, and dropped again before teardown.
fn unique_provider() -> String {
    format!("registry-fixture-{}", afd_db::test_util::mint_id())
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_reset_writes_an_explicit_platform_row_copied_from_the_live_default() {
    let fixture = Fixture::create().await;
    let provider = unique_provider();
    let model = unique_model();
    fixture.seed_catalogue(&provider, &model, 96_000).await;
    let platform_default = fixture
        .seed_platform_default(&provider, &model, 96_000)
        .await;
    let store = providers(&fixture);

    let default = store
        .platform_default()
        .await
        .expect("the default reads")
        .expect("this test seeded an active row");

    // What the reset verb does with it: an EXPLICIT platform row for the
    // tenant, rather than deleting theirs — which is what lets a dashboard tell
    // "reset on purpose" from "never configured".
    store
        .upsert(
            &fixture.tenant,
            &Selection {
                posture: Posture::Platform,
                provider: default.provider.clone(),
                model: default.model.clone(),
                context_cap_tokens: default.context_cap_tokens,
                secret_ref: None,
            },
            NOW,
        )
        .await
        .expect("the platform selection writes");

    let written = store
        .selection(&fixture.tenant)
        .await
        .expect("the selection reads")
        .expect("the reset wrote a row rather than removing one");
    assert_eq!(written.posture, Posture::Platform);
    assert_eq!(written.provider, default.provider);
    assert_eq!(written.model, default.model);
    assert_eq!(written.context_cap_tokens, default.context_cap_tokens);
    assert!(
        written.secret_ref.is_none(),
        "a platform row names no credential of the tenant's"
    );

    drop(platform_default);
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_tenant_that_never_configured_a_provider_is_composed_from_the_live_default() {
    let fixture = Fixture::create().await;
    let provider = unique_provider();
    let model = unique_model();
    fixture.seed_catalogue(&provider, &model, 64_000).await;
    let platform_default = fixture
        .seed_platform_default(&provider, &model, 64_000)
        .await;
    let store = providers(&fixture);

    // The two halves the view is composed from. `None` here is the fact that
    // makes this tenant the "never configured" one — the surface renders it
    // differently from an explicit platform row, which is the only reason the
    // reset writes one at all.
    assert!(
        store
            .selection(&fixture.tenant)
            .await
            .expect("the selection reads")
            .is_none(),
        "a fresh tenant has configured nothing"
    );
    let shown = store
        .platform_default()
        .await
        .expect("the default reads")
        .expect("and the deployment has a default to show it instead");

    // Which is what makes the view a 200 rather than a 404: nothing is
    // missing, the tenant simply has not chosen, and the daemon has something
    // to render.
    //
    // What is asserted is deliberately only the SHAPE. This table has no
    // tenant column, the read is `WHERE active = true ... LIMIT 1`, and this
    // lane's suites run in parallel — a sibling can win the LIMIT 1, and an
    // earlier draft that cross-checked the served values against the table
    // raced the sibling's own cleanup DELETE between the two reads. Every
    // stronger claim inherently does. The exact-value half — that the view
    // renders the seeded row unmodified — is pinned by the daemon walk in
    // `agentsfleetd`'s `integration_tenant_registry`, whose scenario boots
    // against a database of its own and cannot be photobombed.
    assert!(
        !shown.provider.is_empty() && !shown.model.is_empty(),
        "whichever default won the LIMIT 1, it renders as a real row"
    );

    // NOT `resolve()`: that dials, so it needs the platform's own vault key and
    // answers `ProviderSecretMissing` without one. Resolution is graded by
    // `provider_resolution.rs`; what the view needs is the two reads above.

    drop(platform_default);
    fixture.cleanup().await;
}
