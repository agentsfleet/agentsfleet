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

use std::fmt::Debug;

/// Slack's signature scheme.
const SLACK_SOURCE: &str = "slack";
/// See [`SLACK_SOURCE`].
const SLACK_SIGNATURE_HEADER: &str = "x-slack-signature";
/// See [`SLACK_SOURCE`].
const SLACK_TIMESTAMP_HEADER: &str = "x-slack-request-timestamp";
/// See [`SLACK_SOURCE`].
const SLACK_PREFIX: &str = "v0=";

/// GitHub's signature scheme.
const GITHUB_SOURCE: &str = "github";
/// See [`GITHUB_SOURCE`].
const GITHUB_SIGNATURE_HEADER: &str = "x-hub-signature-256";
/// See [`GITHUB_SOURCE`].
const GITHUB_PREFIX: &str = "sha256=";

/// Linear's signature scheme.
const LINEAR_SOURCE: &str = "linear";
/// See [`LINEAR_SOURCE`].
const LINEAR_SIGNATURE_HEADER: &str = "linear-signature";

/// A scheme that carries its digest with no prefix.
const NO_PREFIX: &str = "";

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

/// One provider's scheme, as declared data.
///
/// The shape a crate-backed implementation converges on: `slack-morphism`'s
/// verifier and `octocrab`'s webhook types both ultimately answer these same
/// three strings, so an adapter over either constructs one of these rather
/// than re-implementing the trait from scratch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scheme {
    /// See [`WebhookProvider::source`].
    source: &'static str,
    /// See [`WebhookProvider::signature_header`].
    signature_header: &'static str,
    /// See [`WebhookProvider::signature_prefix`].
    signature_prefix: &'static str,
    /// See [`WebhookProvider::timestamp_header`].
    timestamp_header: Option<&'static str>,
}

impl WebhookProvider for Scheme {
    fn source(&self) -> &str {
        self.source
    }

    fn signature_header(&self) -> &str {
        self.signature_header
    }

    fn signature_prefix(&self) -> &str {
        self.signature_prefix
    }

    fn timestamp_header(&self) -> Option<&str> {
        self.timestamp_header
    }
}

/// Slack: a versioned digest over a timestamped basestring.
pub const SLACK: Scheme = Scheme {
    source: SLACK_SOURCE,
    signature_header: SLACK_SIGNATURE_HEADER,
    signature_prefix: SLACK_PREFIX,
    timestamp_header: Some(SLACK_TIMESTAMP_HEADER),
};

/// GitHub: a prefixed SHA-256 digest over the raw body.
pub const GITHUB: Scheme = Scheme {
    source: GITHUB_SOURCE,
    signature_header: GITHUB_SIGNATURE_HEADER,
    signature_prefix: GITHUB_PREFIX,
    timestamp_header: None,
};

/// Linear: a bare digest over the raw body.
pub const LINEAR: Scheme = Scheme {
    source: LINEAR_SOURCE,
    signature_header: LINEAR_SIGNATURE_HEADER,
    signature_prefix: NO_PREFIX,
    timestamp_header: None,
};

/// The providers this daemon can complete a signature block from.
///
/// Resolution is by `source` and nothing else — a linear scan over three
/// entries, which beats a map at this size and stays deterministic in a way a
/// hash iteration would not.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct StaticRegistry;

/// Every declared scheme, in resolution order.
const SCHEMES: [Scheme; 3] = [SLACK, GITHUB, LINEAR];

impl ProviderRegistry for StaticRegistry {
    fn resolve(&self, source: &str) -> Option<&dyn WebhookProvider> {
        SCHEMES
            .iter()
            .find(|scheme| scheme.source == source)
            .map(|scheme| scheme as &dyn WebhookProvider)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{ProviderRegistry as _, SCHEMES, StaticRegistry};

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
    fn an_undeclared_source_resolves_to_nothing() {
        assert!(
            StaticRegistry.resolve("jira").is_none(),
            "an undeclared provider must not silently borrow another's scheme"
        );
    }

    #[test]
    fn every_source_is_declared_once() {
        for (index, scheme) in SCHEMES.iter().enumerate() {
            let duplicate = SCHEMES
                .iter()
                .skip(index + 1)
                .any(|other| other.source == scheme.source);

            assert!(!duplicate, "`{}` is declared twice", scheme.source);
        }
    }

    #[test]
    fn every_signature_header_is_unique() {
        for (index, scheme) in SCHEMES.iter().enumerate() {
            let duplicate = SCHEMES
                .iter()
                .skip(index + 1)
                .any(|other| other.signature_header == scheme.signature_header);

            assert!(
                !duplicate,
                "`{}` shares a signature header, making detection ambiguous",
                scheme.source
            );
        }
    }

    #[test]
    fn no_scheme_is_declared_with_an_empty_source_or_header() {
        for scheme in &SCHEMES {
            assert!(!scheme.source.is_empty(), "a scheme needs a source");
            assert!(
                !scheme.signature_header.is_empty(),
                "`{}` needs a signature header",
                scheme.source
            );
        }
    }
}
