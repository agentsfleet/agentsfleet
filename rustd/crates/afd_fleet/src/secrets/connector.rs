//! Which connectors this daemon can resolve a credential for.
//!
//! # A connector is a descriptor, not a branch
//!
//! That is `credentials/integration.zig`'s own rule and it is the reason this
//! is a registry rather than an enum with a `match` per question. An enum reads
//! well at five connectors and costs three edits at six — the id, the spelling,
//! and the supply — each in a different function, each easy to get right and
//! easy to forget. A descriptor is one value in one list.
//!
//! The shape is [`afd_fleet_runtime::provider`]'s, deliberately: that module
//! solves the same problem for webhook signature schemes — declared data now, a
//! crate-backed implementation later, one trait so callers cannot tell the
//! difference. Two registries in one workspace answering the same kind of
//! question should not be two different designs.
//!
//! # Where the mint attaches
//!
//! Not here. [`Connector`] answers what the LEASE path asks — what is this
//! called, and does the runner receive its credential or come back for one. The
//! exchange itself (a GitHub App JWT, an OAuth refresh round trip) belongs to
//! the broker behind `POST /credentials/mint`, and it arrives as a second trait
//! that a descriptor or a crate-backed connector implements. Nothing in this
//! file moves when it does, and no caller here learns about it — which is the
//! whole reason the seam is a trait today rather than after the broker lands.
//!
//! `octocrab` is the GitHub client that will back one of these, taken with
//! `default-features = false` for the reason [`afd_fleet_runtime::provider`]
//! records: its defaults pull `rustls-ring`, and this workspace resolves to one
//! crypto provider with `ring` absent from the graph entirely.

use std::fmt::Debug;

use serde_json::Value;

/// The vault-handle field carrying the connector's name.
///
/// Shared with the broker, and the one field the lease path reads out of a
/// stored handle. Everything else in that object is the credential itself.
pub const FIELD_INTEGRATION: &str = "integration";

/// A handle that already carries its token.
const STATIC_NAME: &str = "static";
/// A GitHub App installation token, exchanged from an App JWT.
const GITHUB_NAME: &str = "github";
/// Zoho, through a refresh-token exchange.
const ZOHO_NAME: &str = "zoho";
/// Jira, through a refresh-token exchange.
const JIRA_NAME: &str = "jira";
/// Linear, through a refresh-token exchange.
const LINEAR_NAME: &str = "linear";

/// How a connector's credential reaches the runner.
///
/// Two arms rather than a `bool`, because the two answers are not "yes" and
/// "not yes" — they are two different delivery mechanisms, and a reader meeting
/// `supply()` should not have to remember which way round the bool went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Supply {
    /// The stored handle already holds a usable token; it ships as it stands.
    Inline,
    /// The runner comes back to the broker for a short-lived one.
    OnDemand,
}

/// What the lease path needs to know about one connector.
///
/// Object-safe on purpose: a registry holds these as `dyn`, so a declared
/// descriptor and a crate-backed implementation are interchangeable to every
/// caller. Synchronous and pure for the reason [`crate::provider`]'s strategy
/// trait is — nothing here opens a socket, so nothing here needs to be
/// awaited, and the whole classification stays provable with no datastore.
pub trait Connector: Debug + Send + Sync {
    /// The `integration` value a stored handle names this connector by.
    ///
    /// Also the value stored in `core.integration_grants.service` and emitted
    /// on the execution policy, so it is one audited string rather than
    /// whatever a variant rename produces.
    fn name(&self) -> &str;

    /// How its credential reaches the runner.
    fn supply(&self) -> Supply;
}

/// Where a stored handle's `integration` value is resolved.
///
/// A trait rather than a free function over a constant, so the daemon can hold
/// one registry and a test can hold another — which is what
/// `mintsOnDemand(registry, id)` takes its registry parameter for, and what an
/// operator-configured connector set would implement without this module
/// knowing it exists.
pub trait Connectors: Debug + Send + Sync {
    /// The connector `name` names, if this registry has one.
    fn resolve(&self, name: &str) -> Option<&dyn Connector>;
}

/// One connector's contract, as declared data.
///
/// The shape a crate-backed implementation converges on: whatever client
/// eventually performs a GitHub exchange still answers these same two
/// questions, so an adapter over it constructs one of these rather than
/// re-implementing the trait from scratch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Descriptor {
    /// See [`Connector::name`].
    name: &'static str,
    /// See [`Connector::supply`].
    supply: Supply,
}

impl Connector for Descriptor {
    fn name(&self) -> &str {
        self.name
    }

    fn supply(&self) -> Supply {
        self.supply
    }
}

/// A stored personal access token or equivalent, usable as it stands.
pub const STATIC: Descriptor = Descriptor {
    name: STATIC_NAME,
    supply: Supply::Inline,
};

/// GitHub: an App JWT exchanged for a short-lived installation token.
pub const GITHUB: Descriptor = Descriptor {
    name: GITHUB_NAME,
    supply: Supply::OnDemand,
};

/// Zoho: a refresh token exchanged for a short-lived access token.
pub const ZOHO: Descriptor = Descriptor {
    name: ZOHO_NAME,
    supply: Supply::OnDemand,
};

/// Jira: a refresh token exchanged for a short-lived access token.
pub const JIRA: Descriptor = Descriptor {
    name: JIRA_NAME,
    supply: Supply::OnDemand,
};

/// Linear: a refresh token exchanged for a short-lived access token.
pub const LINEAR: Descriptor = Descriptor {
    name: LINEAR_NAME,
    supply: Supply::OnDemand,
};

/// Every connector this daemon declares, in resolution order.
///
/// Adding one is ONE entry here. The api-key connectors — datadog, grafana, fly
/// — are deliberately absent: their key is used directly and never reaches a
/// broker, so a row for them would be a row nothing dispatches on. They resolve
/// to nothing and take the same fail-safe path a typo does, which is what
/// [`mintable`] is careful about.
const DECLARED: [Descriptor; 5] = [STATIC, GITHUB, ZOHO, JIRA, LINEAR];

/// The connector set this daemon ships with.
///
/// Resolution is by name and nothing else — a linear scan over five entries,
/// which beats a map at this size and stays deterministic in a way a hash
/// iteration would not.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct Registry;

impl Connectors for Registry {
    fn resolve(&self, name: &str) -> Option<&dyn Connector> {
        DECLARED
            .iter()
            .find(|declared| declared.name == name)
            .map(|declared| declared as &dyn Connector)
    }
}

/// The connector a stored handle must be MINTED through, if any.
///
/// `None` means the credential ships as its stored value, and it is the answer
/// for every case that is not unambiguously a registered on-demand connector: a
/// handle that is not an object, one with no `integration` field, one whose
/// field is not a string, one naming something this registry does not carry,
/// and one naming an inline connector.
///
/// That breadth is deliberate and it is the direction the Zig fails in too.
/// Falling through to a stored value costs a credential that could have been
/// short-lived; falling through to a mint marker costs the runner a credential
/// it never receives.
#[must_use]
pub fn mintable<'a>(connectors: &'a dyn Connectors, handle: &Value) -> Option<&'a dyn Connector> {
    handle
        .as_object()?
        .get(FIELD_INTEGRATION)?
        .as_str()
        .and_then(|name| connectors.resolve(name))
        .filter(|connector| connector.supply() == Supply::OnDemand)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{Connector, Connectors as _, DECLARED, Registry, Supply, mintable};
    use serde_json::json;

    /// A registry declaring one connector under a name the production one does
    /// not carry — the injected-fake seam the trait exists for.
    #[derive(Debug)]
    struct OneOff(super::Descriptor);

    impl super::Connectors for OneOff {
        fn resolve(&self, name: &str) -> Option<&dyn Connector> {
            (self.0.name == name).then_some(&self.0 as &dyn Connector)
        }
    }

    #[test]
    fn a_registered_on_demand_handle_resolves_to_its_connector() {
        let mintable = mintable(
            &Registry,
            &json!({"integration": "github", "installation_id": "42", "app_id": "7"}),
        )
        .expect("github is declared and mints on demand");

        assert_eq!(mintable.name(), "github");
        assert_eq!(mintable.supply(), Supply::OnDemand);
    }

    #[test]
    fn everything_ambiguous_falls_through_to_a_stored_value() {
        // Each of these is a separate way this can answer "not mintable", and
        // the reason they are one test is that they must all keep answering the
        // SAME way — a fail-safe that is safe for six of seven inputs is not a
        // fail-safe.
        for stored in [
            // No integration field: an ordinary stored credential.
            json!({"api_token": "FlyTokenXyz"}),
            // Registered, but delivered inline.
            json!({"integration": "static", "token": "ghp_abc"}),
            // An api-key connector, deliberately undeclared.
            json!({"integration": "datadog", "token": "z"}),
            // A typo.
            json!({"integration": "githubb"}),
            // The field is not a string.
            json!({"integration": 7}),
            // The handle is not an object.
            json!(["github"]),
            json!(null),
        ] {
            assert!(mintable(&Registry, &stored).is_none(), "{stored}");
        }
    }

    #[test]
    fn a_test_registry_answers_where_the_shipped_one_does_not() {
        // The property an injected registry buys: a connector under test needs
        // no entry in the daemon's own list, so a test cannot pass by
        // accidentally depending on what production ships.
        let fake = OneOff(super::Descriptor {
            name: "acme",
            supply: Supply::OnDemand,
        });

        assert!(mintable(&Registry, &json!({"integration": "acme"})).is_none());
        assert_eq!(
            mintable(&fake, &json!({"integration": "acme"}))
                .expect("the fake declares it")
                .name(),
            "acme"
        );
        // And the fake does NOT silently answer for the shipped set.
        assert!(mintable(&fake, &json!({"integration": "github"})).is_none());
    }

    #[test]
    fn every_declared_name_resolves_back_to_itself() {
        // The Zig proves this in a `comptime` block over `toString` /
        // `idFromString`. Here the name IS the descriptor's field, so what is
        // worth proving is that the registry can find every entry it declares —
        // a descriptor absent from `DECLARED` is one nothing can ever resolve.
        for declared in &DECLARED {
            let resolved = Registry
                .resolve(declared.name)
                .expect("a declared connector resolves");
            assert_eq!(resolved.name(), declared.name);
            assert_eq!(resolved.supply(), declared.supply);
        }
    }

    #[test]
    fn no_name_is_declared_twice_and_none_is_empty() {
        for (index, declared) in DECLARED.iter().enumerate() {
            assert!(!declared.name.is_empty(), "a connector needs a name");
            assert!(
                !DECLARED
                    .iter()
                    .skip(index + 1)
                    .any(|other| other.name == declared.name),
                "`{}` is declared twice, so resolution picks one arbitrarily",
                declared.name
            );
        }
    }

    #[test]
    fn only_a_static_handle_is_delivered_inline() {
        // The default direction, stated as a test: a connector added without
        // thought about delivery must not ship a stored refresh token to a
        // child process.
        for declared in &DECLARED {
            let expected = if declared.name == "static" {
                Supply::Inline
            } else {
                Supply::OnDemand
            };
            assert_eq!(declared.supply, expected, "{}", declared.name);
        }
    }
}
