//! Which provider a webhook trigger speaks to, and what its signature scheme
//! is called.
//!
//! # What this module is, and firmly is not
//!
//! It is the SEAM. A [`WebhookProvider`] answers three pieces of metadata —
//! the header a signature arrives in, the prefix that header's value carries,
//! and the timestamp header when the scheme binds one — so a trigger that
//! declares `signature: { secret_ref: … }` and nothing else can be completed
//! from what the provider already knows. That is the whole of what CONFIG
//! PARSING needs from a provider.
//!
//! It is not a client. Nothing here opens a socket, and nothing here verifies
//! a signature. `webhook_verify.zig` fuses the two — the same const table holds
//! the header names and the HMAC comparison — and the cost is that a pure
//! config parse drags the verification path in behind it.
//!
//! # Where the provider CRATES go, and why not here
//!
//! Real, maintained crates exist for two of these providers and were checked
//! rather than recalled:
//!
//! - **`octocrab`** (0.54.1, active) is the GitHub client. It must be taken
//!   with `default-features = false`: its defaults enable `rustls-ring` and
//!   `jwt-rust-crypto`, and this workspace resolves to ONE crypto provider —
//!   `aws-lc-rs`, with `ring` absent from the graph entirely. `octocrab` ships
//!   `rustls-aws-lc-rs` and `jwt-aws-lc-rs` for exactly this, so the constraint
//!   costs a feature list rather than the crate.
//! - **`slack-morphism`** (2.25.0, active) carries a real signature verifier
//!   behind its `signature-verifier` feature.
//!
//! Neither belongs in a config parser. Both are asynchronous HTTP clients, and
//! this crate is called on the claim path to turn stored JSON into a policy —
//! putting a client behind that is weight on a hot path for metadata a client
//! does not even expose as a constant. They belong at the sites that make the
//! CALL: the credential mint, the repair-verification dispatcher, and the
//! connector outbound worker. This trait is what lets them land there without
//! this module moving.
//!
//! **Linear, Jira and Zoho have no crate worth taking**, and that was checked
//! too: `linear_sdk` and `jira` are both at `0.0.1`, and `zohohorrorshow`
//! (0.9.0) covers Zoho Projects, not the webhook surfaces here. They stay
//! declared below, and when one of them grows a maintained client it
//! implements this trait rather than editing a table.
//!
//! # The strings themselves live in `afd_webhook`
//!
//! [`Scheme`] states the header, the prefix and the timestamp header once, for
//! the verifier that reads them. This module resolves a `source` to one of
//! those values rather than declaring a second copy: a registry whose header
//! disagreed with the wall's would complete a trigger the wall then refuses,
//! and the refusal reads as a wrong secret rather than a wrong header.

use std::fmt::Debug;

use afd_webhook::Scheme;

/// What a provider knows about its own webhook signature scheme.
///
/// Object-safe on purpose: a registry holds these as `dyn`, so a
/// crate-backed implementation and a declared one are interchangeable to
/// every caller.
pub trait WebhookProvider: Debug + Send + Sync {
    /// The `source` an authored trigger names this provider by.
    fn source(&self) -> &str;

    /// The header a signature arrives in.
    ///
    /// Unique across a registry — two providers sharing one header would make
    /// detection by header ambiguous, which is why
    /// [`StaticRegistry`] proves the property rather than assuming it.
    fn signature_header(&self) -> &str;

    /// What the header's value is prefixed with, or empty when it carries the
    /// digest bare.
    fn signature_prefix(&self) -> &str;

    /// The header carrying the signed timestamp, for schemes that bind one.
    ///
    /// `None` is a scheme with no replay binding of its own, not an unknown.
    fn timestamp_header(&self) -> Option<&str>;
}

/// Where a trigger's provider defaults are resolved from.
///
/// A trait rather than a function so the daemon can hold one registry value
/// and a test can hold another — and so the crate-backed implementations
/// described in the module documentation can be composed in without this
/// module knowing they exist.
pub trait ProviderRegistry: Debug + Send + Sync {
    /// The provider a trigger's `source` names, if this registry has one.
    fn resolve(&self, source: &str) -> Option<&dyn WebhookProvider>;
}

/// The providers this daemon can complete a signature block from.
///
/// Resolution is by `source` and nothing else — a linear scan over three
/// entries, which beats a map at this size and stays deterministic in a way a
/// hash iteration would not.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct StaticRegistry;

/// The signature wall's own enum answers this trait directly.
///
/// No adapter struct in between, because there is nothing to adapt: the four
/// strings this trait asks for are the four `Scheme` already states, and a
/// second declaration of them is the drift this impl exists to make
/// impossible.
impl WebhookProvider for Scheme {
    fn source(&self) -> &str {
        Self::source(*self)
    }

    fn signature_header(&self) -> &str {
        Self::signature_header(*self)
    }

    fn signature_prefix(&self) -> &str {
        Self::prefix(*self)
    }

    fn timestamp_header(&self) -> Option<&str> {
        Self::timestamp_header(*self)
    }
}

impl ProviderRegistry for StaticRegistry {
    fn resolve(&self, source: &str) -> Option<&dyn WebhookProvider> {
        Scheme::ALL
            .iter()
            .find(|scheme| scheme.source() == source)
            .map(|scheme| scheme as &dyn WebhookProvider)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{ProviderRegistry as _, Scheme, StaticRegistry, WebhookProvider as _};

    #[test]
    fn a_declared_source_resolves_to_its_scheme() {
        let resolved = StaticRegistry
            .resolve("github")
            .expect("github is declared");

        assert_eq!(resolved.signature_header(), "x-hub-signature-256");
        assert_eq!(resolved.signature_prefix(), "sha256=");
        assert_eq!(
            resolved.timestamp_header(),
            None,
            "github binds no timestamp of its own"
        );
    }

    #[test]
    fn slack_carries_the_timestamp_header_its_basestring_signs() {
        let resolved = StaticRegistry.resolve("slack").expect("slack is declared");

        assert_eq!(
            resolved.timestamp_header(),
            Some("x-slack-request-timestamp")
        );
    }

    #[test]
    fn the_bare_arm_resolves_with_no_prefix_and_no_timestamp() {
        let resolved = StaticRegistry.resolve("linear").expect("linear is declared");

        assert_eq!(resolved.signature_header(), "linear-signature");
        assert_eq!(
            resolved.signature_prefix(),
            "",
            "carrying no prefix is a property of the scheme, not an empty value in a row"
        );
        assert_eq!(resolved.timestamp_header(), None);
    }

    #[test]
    fn an_undeclared_source_resolves_to_nothing() {
        assert!(
            StaticRegistry.resolve("jira").is_none(),
            "an undeclared provider must not silently borrow another's scheme"
        );
    }

    #[test]
    fn every_source_is_declared_once() {
        for (index, scheme) in Scheme::ALL.iter().enumerate() {
            let duplicate = Scheme::ALL
                .iter()
                .skip(index + 1)
                .any(|other| other.source() == scheme.source());

            assert!(!duplicate, "`{}` is declared twice", scheme.source());
        }
    }

    #[test]
    fn every_signature_header_is_unique() {
        for (index, scheme) in Scheme::ALL.iter().enumerate() {
            let duplicate = Scheme::ALL
                .iter()
                .skip(index + 1)
                .any(|other| other.signature_header() == scheme.signature_header());

            assert!(
                !duplicate,
                "`{}` shares a signature header, making detection ambiguous",
                scheme.source()
            );
        }
    }

    #[test]
    fn no_scheme_is_declared_with_an_empty_source_or_header() {
        for scheme in Scheme::ALL {
            assert!(!scheme.source().is_empty(), "a scheme needs a source");
            assert!(
                !scheme.signature_header().is_empty(),
                "`{}` needs a signature header",
                scheme.source()
            );
        }
    }
}
