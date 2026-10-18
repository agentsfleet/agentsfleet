//! The three renderings the provider view is composed from.
//!
//! Split out of [`super`] rather than written in it: that file is at the length
//! cap, and its remaining half is three async handlers whose refusals are the
//! router suite's to prove. What is here is the part that decides a SHAPE from
//! values already read — what an own entry carries, which fields the platform
//! default carries, and what "nothing configured" looks like — and none of it
//! needs a pool to reach.
//!
//! The header of [`super`] names the composition these three implement:
//!
//!   own entry           → the entry, rendered as self-managed
//!   platform + default  → the live default, rendered as platform mode
//!   platform, no default → the empty view — "not configured", never a 404
//!
//! One test per rung, so a rung that started rendering as another is a named
//! failure rather than a diff in a larger assertion.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use afd_core::id::Uuid7;

use super::*;

/// The context ceiling the fixture's live default carries.
///
/// Named because the fallback's value and the assertion on it are one fact: a
/// literal spelled on both sides can drift on one and still read as a pass.
const DEFAULT_CAP_TOKENS: u32 = 1_000_000;

/// The workspace a seeded default is sourced from.
fn workspace() -> Uuid7 {
    Uuid7::parse("019329c5-0000-7000-8000-0000000000b1")
        .expect("the fixture workspace id is canonical")
}

/// A tenant's own entry, its credential naming `provider`.
fn own(provider: Option<&str>) -> OwnModel {
    OwnModel {
        tenant_model_id: Uuid7::parse("019329c5-0000-7000-8000-0000000000c1")
            .expect("the fixture entry id is canonical"),
        model: "gpt-5.1".into(),
        secret_ref: "openai-prod".into(),
        provider: provider.map(Into::into),
        context_cap_tokens: 400_000,
    }
}

/// An own entry renders as self-managed, and keeps the credential it names.
///
/// The `secret_ref` is the half a client acts on — it is what the Models page
/// shows as "your key" — so a mode that survived a rename while the reference
/// was dropped would still look right in the mode field alone.
#[test]
fn should_render_an_own_entry_with_the_credential_it_names() {
    let entry = own(Some("openai"));

    let view = from_own(&entry, true);

    assert_eq!(view.mode, ProviderMode::SelfManaged);
    assert_eq!(view.provider, "openai");
    assert_eq!(view.model, "gpt-5.1");
    assert_eq!(view.context_cap_tokens, 400_000);
    assert_eq!(view.secret_ref, Some("openai-prod"));
    assert!(view.platform_default_available);
}

/// An entry whose credential names no provider renders the empty name, and
/// passes availability through rather than inferring it.
#[test]
fn should_render_an_entry_whose_credential_names_no_provider_with_the_empty_name() {
    let entry = own(None);

    let view = from_own(&entry, false);

    assert_eq!(view.mode, ProviderMode::SelfManaged);
    assert_eq!(view.provider, NOT_CONFIGURED);
    assert_eq!(view.secret_ref, Some("openai-prod"));
    assert!(
        !view.platform_default_available,
        "availability is read independently and passed through, not inferred \
         from the entry"
    );
}

/// A tenant with no row of its own is composed from the live default, rendered
/// as platform mode and naming no credential.
///
/// `platform_default_available` is passed rather than hardcoded true even
/// though this rung is only reached WHEN a default exists — the flag is the
/// caller's fact, and a renderer that decided it for itself would be a second
/// place for the two to disagree.
#[test]
fn should_compose_a_tenant_without_a_row_from_the_live_default() {
    let fallback = PlatformDefault {
        provider: "anthropic".into(),
        source_workspace_id: workspace(),
        model: "claude-sonnet-5".into(),
        base_url: None,
        context_cap_tokens: DEFAULT_CAP_TOKENS,
    };

    let view = from_default(&fallback, true);

    assert_eq!(view.mode, ProviderMode::Platform);
    assert_eq!(view.provider, "anthropic");
    assert_eq!(view.model, "claude-sonnet-5");
    assert_eq!(view.context_cap_tokens, DEFAULT_CAP_TOKENS);
    assert_eq!(
        view.secret_ref, None,
        "the platform's own key is never a tenant's credential reference"
    );
    assert!(view.platform_default_available);
}

/// Nothing configured anywhere is an empty view, never a 404.
///
/// Empty strings, deliberately: a client that got a 404 would have to tell
/// "this deployment has no default" apart from "this route is gone", and the
/// Models page renders the empty names as its "not configured" state.
#[test]
fn should_render_the_empty_view_when_nothing_is_configured_anywhere() {
    let view = empty_view();

    assert_eq!(view.mode, ProviderMode::Platform);
    assert_eq!(view.provider, NOT_CONFIGURED);
    assert_eq!(view.model, NOT_CONFIGURED);
    assert_eq!(view.context_cap_tokens, 0);
    assert_eq!(view.secret_ref, None);
    assert!(
        !view.platform_default_available,
        "there is no default to switch to, which is what this rung means"
    );
}
