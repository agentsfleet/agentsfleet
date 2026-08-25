//! Which credentials the runner is handed, and which it must come back for.
//!
//! # What this module is, and what it is NOT
//!
//! It is the CLASSIFICATION half of `credentials/integration.zig`: the ids, the
//! wire spellings, and the one question the lease path asks — does this
//! credential resolve by an on-demand mint, or is its stored value usable as it
//! stands?
//!
//! It is not the broker. Nothing here signs an App JWT, posts to a token
//! endpoint, or holds a `MintCtx`. Those are the `Mint` strategies, and they
//! belong to the credential-mint broker behind `POST /credentials/mint` — a
//! verb the lease path never calls. Porting them here would drag an
//! asynchronous HTTP client into the claim path for metadata that is three
//! strings and a bool.
//!
//! # Why there is no `REGISTRY` slice
//!
//! The Zig needs one because its `Spec` carries a FUNCTION POINTER: `resolve`
//! is a linear scan for a mint strategy, and `mintsOnDemand` takes the registry
//! as a parameter so a test can inject a fake. Strip the strategies out — which
//! is what this milestone's scope does — and every entry collapses to a bool
//! that is a property of the id itself. A table would then be a lookup whose
//! answer the type already knows, and an injected registry would be a seam for
//! a test that does not need one: "github mints on demand" is total over a
//! closed enum, not a configuration.
//!
//! When the broker lands, the strategies attach as a trait — `Mint::run` is a
//! tagged union with a `run` method, which is a trait in every language that
//! has one — and the id stays what it is here: the key that selects one.

use serde::Deserialize;
use serde_json::Value;

/// The vault-handle field carrying the integration id.
///
/// Shared with the broker, and the one field the lease path reads out of a
/// stored handle. Everything else in that object is the credential itself.
pub const FIELD_INTEGRATION: &str = "integration";

/// A connector the daemon knows how to resolve a credential for.
///
/// The api-key connectors — datadog, grafana, fly — are deliberately absent:
/// their key is used directly and never reaches a broker, so an id for them
/// would be an id nothing dispatches on. An unknown spelling therefore lands on
/// the same fail-safe path they do, which is what [`mintable`] is careful about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Integration {
    /// The handle already carries a usable token. Resolved inline, never minted.
    Static,
    /// A GitHub App installation token, exchanged from an App JWT.
    Github,
    /// Zoho, through a refresh-token exchange.
    Zoho,
    /// Jira, through a refresh-token exchange.
    Jira,
    /// Linear, through a refresh-token exchange.
    Linear,
}

impl Integration {
    /// The canonical wire and column spelling.
    ///
    /// Written out rather than derived from the variant name, which is what the
    /// Zig's `toString` does and for the reason it gives: this value is stored
    /// in `core.integration_grants.service` and compared against it, so it
    /// wants one audited source rather than whatever a rename produces.
    /// [`Integration::parse`] round-trips it, and a test proves that for every
    /// variant — so a rename is a failing test rather than a silent grant-check
    /// miss.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Github => "github",
            Self::Zoho => "zoho",
            Self::Jira => "jira",
            Self::Linear => "linear",
        }
    }

    /// Recover an integration from its stored spelling.
    ///
    /// `None` for anything this daemon does not register, which includes both a
    /// typo and a connector whose key is used directly. Both are treated the
    /// same way by the only caller, and that is the safe treatment — see
    /// [`mintable`].
    #[must_use]
    pub fn parse(stored: &str) -> Option<Self> {
        [
            Self::Static,
            Self::Github,
            Self::Zoho,
            Self::Jira,
            Self::Linear,
        ]
        .into_iter()
        .find(|candidate| candidate.as_str() == stored)
    }

    /// Whether the runner must come back to the broker for this credential.
    ///
    /// Only [`Integration::Static`] is inline — its handle already holds the
    /// token — so every other integration defers to a mint. Stated as one
    /// negation rather than as a list, so a new connector is on-demand by
    /// DEFAULT: the failure mode of the other spelling is shipping a stored
    /// refresh token to a child process, and the failure mode of this one is a
    /// mint that answers `unknown_integration`.
    #[must_use]
    pub const fn mints_on_demand(self) -> bool {
        !matches!(self, Self::Static)
    }
}

/// The integration a stored handle must be MINTED through, if any.
///
/// `None` means the credential ships as its stored value, and it is the answer
/// for every case that is not unambiguously a registered on-demand connector: a
/// handle that is not an object, one with no `integration` field, one whose
/// field is not a string, one naming something unregistered, and one naming
/// `static`. That breadth is deliberate and it is the direction the Zig fails
/// in too — falling through to a stored value costs a credential that could
/// have been short-lived, while falling through to a mint marker costs the
/// runner a credential it never receives.
#[must_use]
pub fn mintable(handle: &Value) -> Option<Integration> {
    handle
        .as_object()?
        .get(FIELD_INTEGRATION)?
        .as_str()
        .and_then(Integration::parse)
        .filter(|integration| integration.mints_on_demand())
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{Integration, mintable};
    use serde_json::json;

    #[test]
    fn every_spelling_round_trips() {
        // The property the Zig proves in a `comptime` block: a renamed variant
        // or an edited spelling must not become a grant check that silently
        // matches nothing.
        for integration in [
            Integration::Static,
            Integration::Github,
            Integration::Zoho,
            Integration::Jira,
            Integration::Linear,
        ] {
            assert_eq!(
                Integration::parse(integration.as_str()),
                Some(integration),
                "{integration:?} does not round-trip"
            );
        }
    }

    #[test]
    fn the_serde_spelling_agrees_with_the_written_one() {
        // Two ways to name the same variant is two ways for it to drift. The
        // wire spelling is `as_str`; this pins the derive to it, so a handle
        // deserialized through serde and one classified through `parse` cannot
        // disagree.
        for integration in [Integration::Github, Integration::Static] {
            let deserialized: Integration =
                serde_json::from_value(serde_json::Value::String(integration.as_str().to_owned()))
                    .expect("the written spelling deserializes");
            assert_eq!(deserialized, integration);
        }
    }

    #[test]
    fn only_a_static_handle_carries_its_own_token() {
        assert!(!Integration::Static.mints_on_demand());
        for on_demand in [
            Integration::Github,
            Integration::Zoho,
            Integration::Jira,
            Integration::Linear,
        ] {
            assert!(on_demand.mints_on_demand(), "{on_demand:?}");
        }
    }

    #[test]
    fn a_registered_on_demand_handle_classifies_as_mintable() {
        assert_eq!(
            mintable(&json!({"integration": "github", "installation_id": "42", "app_id": "7"})),
            Some(Integration::Github)
        );
        assert_eq!(
            mintable(&json!({"integration": "zoho", "refresh_token": "rt"})),
            Some(Integration::Zoho)
        );
    }

    #[test]
    fn everything_ambiguous_falls_through_to_a_stored_value() {
        // Each of these is a separate way the Zig's `mintableId` answers null,
        // and the reason they are one test is that they must all keep answering
        // the SAME way — a fail-safe that is safe for four of five inputs is
        // not a fail-safe.
        for stored in [
            // No integration field at all: an ordinary stored credential.
            json!({"api_token": "FlyTokenXyz"}),
            // Registered, but resolved inline.
            json!({"integration": "static", "token": "ghp_abc"}),
            // An api-key connector, deliberately unregistered.
            json!({"integration": "datadog", "token": "z"}),
            // A typo.
            json!({"integration": "githubb"}),
            // The field is not a string.
            json!({"integration": 7}),
            // The handle is not an object.
            json!(["github"]),
            json!("github"),
            json!(null),
        ] {
            assert_eq!(mintable(&stored), None, "{stored}");
        }
    }
}
