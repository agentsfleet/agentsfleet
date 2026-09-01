//! The three renderings the provider view is composed from.
//!
//! Split out of [`super`] rather than written in it: that file is at the length
//! cap, and its remaining half is three async handlers whose refusals are the
//! router suite's to prove. What is here is the part that decides a SHAPE from
//! values already read — which mode a posture spells, which fields a fallback
//! carries, and what "nothing configured" looks like — and none of it needs a
//! pool to reach.
//!
//! The header of [`super`] names the composition these three implement:
//!
//!   row?                → the stored row, whatever its mode
//!   no row + default    → the live default, rendered as platform mode
//!   no row + no default → the empty view — "not configured", never a 404
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

/// A stored selection in `posture`, naming a credential where one applies.
fn selection(posture: Posture, secret_ref: Option<&str>) -> Selection {
    Selection {
        posture,
        provider: "openai".into(),
        model: "gpt-5.1".into(),
        context_cap_tokens: 400_000,
        secret_ref: secret_ref.map(Into::into),
    }
}

/// A self-managed row renders as self-managed, and keeps the credential it
/// names.
///
/// The `secret_ref` is the half a client acts on — it is what the Models page
/// shows as "your key" — so a mode that survived a rename while the reference
/// was dropped would still look right in the mode field alone.
#[test]
fn should_render_a_self_managed_row_with_the_credential_it_names() {
    let own = selection(Posture::SelfManaged, Some("openai-prod"));

    let view = from_selection(&own, true);

    assert_eq!(view.mode, ProviderMode::SelfManaged);
    assert_eq!(view.provider, "openai");
    assert_eq!(view.model, "gpt-5.1");
    assert_eq!(view.context_cap_tokens, 400_000);
    assert_eq!(view.secret_ref, Some("openai-prod"));
    assert!(view.platform_default_available);
}

/// An explicit platform row renders as platform mode and carries no credential.
///
/// The pair with the case above is the point: the same function, the same
/// fields, and the posture is the only input that differs.
#[test]
fn should_render_an_explicit_platform_row_without_a_credential() {
    let own = selection(Posture::Platform, None);

    let view = from_selection(&own, false);

    assert_eq!(view.mode, ProviderMode::Platform);
    assert_eq!(view.secret_ref, None);
    assert!(
        !view.platform_default_available,
        "availability is read independently and passed through, not inferred \
         from the row's own mode"
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
/// The Zig serves empty strings here and this matches it deliberately: a client
/// that got a 404 would have to tell "this deployment has no default" apart
/// from "this route is gone", and the Models page renders the empty names as
/// its "not configured" state.
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
