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

/// How a connector's credential reaches the runner.
///
/// Two arms rather than a `bool`, because the two answers are not "yes" and
/// "not yes" — they are two different delivery mechanisms, and a reader meeting
/// `supply()` should not have to remember which way round the bool went.
///
/// DERIVED from [`Exchange`] rather than declared beside it — see
/// [`Descriptor::supply`]. A descriptor that could state both independently
/// could state them in contradiction.
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

    /// How this connector turns a stored handle into a credential.
    ///
    /// The broker's dispatch reads this and nothing else, which is what keeps
    /// "adding a connector" a row in [`DECLARED`] rather than an arm somewhere
    /// in the broker.
    fn exchange(&self) -> Exchange;

    /// How its credential reaches the runner.
    ///
    /// PROVIDED, and deliberately not overridable in practice: it is derived
    /// from [`Self::exchange`] here so no implementation can state the two in
    /// contradiction — see [`Descriptor::supply`] for what that would cost.
    fn supply(&self) -> Supply {
        match self.exchange() {
            Exchange::Stored => Supply::Inline,
            Exchange::GithubApp | Exchange::OAuthRefresh { .. } => Supply::OnDemand,
        }
    }
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

/// How a connector turns a stored handle into a usable credential.
///
/// # Why the exchange is DATA on the descriptor, and not a `dyn Mint`
///
/// The alternative — a trait object per connector — puts one implementation
/// behind every provider, so a fourth refresh-token provider is a fourth type
/// that reimplements what three others already do. Here it is a fourth ROW.
///
/// Zoho, Jira and Linear differ by exactly one string: the endpoint their
/// refresh grant posts to. That is a field, not a subclass. The broker matches
/// on this enum ONCE, so adding a provider that mints the way an existing one
/// does costs no dispatch code at all, and adding a genuinely new KIND of
/// exchange adds a variant the compiler then forces every match to answer.
///
/// It is also the shape `integration.zig` arrived at independently — a `Spec`
/// whose `mint` field is a union of `static`, a GitHub-App custom mint, and an
/// `oauth2_refresh` carrying its own `token_endpoint`. Two implementations
/// converging on declared-data dispatch is a good sign it is the right one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exchange {
    /// Nothing to exchange: the stored handle already holds a usable token.
    Stored,
    /// A GitHub App JWT exchanged for a repository-scoped installation token.
    ///
    /// Carries no endpoint because it has no per-provider one — the App
    /// installation is addressed by an id out of the stored handle, and the
    /// scoping is the part that varies, which is a property of the FLEET's
    /// binding rather than of the connector.
    GithubApp,
    /// An RFC 6749 §6 `refresh_token` grant, posted to `token_url`.
    OAuthRefresh {
        /// Where the grant is posted. The only thing separating these
        /// providers from one another.
        token_url: &'static str,
    },
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
    /// How this connector turns a handle into a credential.
    exchange: Exchange,
}

impl Descriptor {
    /// How this connector mints, for the broker to dispatch on.
    ///
    /// Inherent AND a trait method: a caller holding a concrete descriptor gets
    /// a `const` answer, and one holding `&dyn Connector` — which is every
    /// caller the registry serves — gets the same value through the trait.
    #[must_use]
    pub const fn exchange(&self) -> Exchange {
        self.exchange
    }
}

impl Connector for Descriptor {
    fn name(&self) -> &str {
        self.name
    }

    fn exchange(&self) -> Exchange {
        self.exchange
    }

    // `supply` is the trait's provided method, derived from `exchange` and
    // never declared alongside it. The two are the same fact asked twice —
    // anything with an exchange to perform is fetched on demand, and the one
    // thing without an exchange ships as it stands. Storing both would let a
    // row say `Stored` and `OnDemand` at once, and the lease path and the
    // broker would then disagree about the same connector.
}

impl Descriptor {
    /// Declares one connector.
    ///
    /// `const` so [`DECLARED`] stays a compile-time table, and private because
    /// the shipped set is this module's to state — a caller wanting a connector
    /// of its own implements [`Connector`], which is the seam that exists for
    /// it.
    const fn new(name: &'static str, exchange: Exchange) -> Self {
        Self { name, exchange }
    }
}

/// Zoho's refresh-grant endpoint.
const ZOHO_TOKEN_URL: &str = "https://accounts.zoho.com/oauth/v2/token";
/// Jira's, which is Atlassian's shared one.
const JIRA_TOKEN_URL: &str = "https://auth.atlassian.com/oauth/token";
/// Linear's.
const LINEAR_TOKEN_URL: &str = "https://api.linear.app/oauth/token";

/// Every connector this daemon declares, in resolution order.
///
/// **Adding one is ONE line here, and that is the whole point of the table.**
/// An earlier shape spelled each entry three times — a name constant, a public
/// descriptor constant, and a row in this list — plus a fourth edit to widen a
/// fixed-length array. That is precisely the per-connector edit count the enum
/// alternative was rejected for in this module's own header, arrived at from
/// the other direction. A slice has no length to keep in step, and a row that
/// carries its own name has no constant to define away from it.
///
/// The api-key connectors — datadog, grafana, fly — are deliberately absent:
/// their key is used directly and never reaches a broker, so a row for them
/// would be a row nothing dispatches on. They resolve to nothing and take the
/// same fail-safe path a typo does, which is what [`mintable`] is careful
/// about.
const DECLARED: &[Descriptor] = &[
    // A stored personal access token or equivalent, usable as it stands.
    Descriptor::new("static", Exchange::Stored),
    // GitHub: an App JWT exchanged for a repository-scoped installation token.
    Descriptor::new("github", Exchange::GithubApp),
    // Zoho, Jira and Linear: one exchange, three providers, differing by a
    // single URL — which is the property that has to survive the next one
    // being added, and the reason the exchange is a field rather than a type.
    Descriptor::new(
        "zoho",
        Exchange::OAuthRefresh {
            token_url: ZOHO_TOKEN_URL,
        },
    ),
    Descriptor::new(
        "jira",
        Exchange::OAuthRefresh {
            token_url: JIRA_TOKEN_URL,
        },
    ),
    Descriptor::new(
        "linear",
        Exchange::OAuthRefresh {
            token_url: LINEAR_TOKEN_URL,
        },
    ),
];

/// The connector set this daemon ships with.
///
/// Resolution is by name and nothing else — a linear scan over five entries,
/// which beats a map at this size and stays deterministic in a way a hash
/// iteration would not.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct Registry;

impl Registry {
    /// Every connector this daemon ships, in declaration order.
    ///
    /// The seam the boot-time platform load walks: a deployment's App and OAuth
    /// clients are one vault row per connector, and iterating the table is what
    /// keeps "adding a connector is ONE line" true through the composition root
    /// as well. A loader that named zoho, jira and linear itself would be the
    /// fourth edit this module exists to avoid.
    pub fn declared(&self) -> impl Iterator<Item = &'static Descriptor> {
        DECLARED.iter()
    }
}

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
    use super::{Connector, Connectors as _, DECLARED, Exchange, Registry, Supply, mintable};
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
        let fake = OneOff(super::Descriptor::new(
            "acme",
            Exchange::OAuthRefresh {
                token_url: "https://accounts.example.test/oauth/token",
            },
        ));

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
        for declared in DECLARED {
            let resolved = Registry
                .resolve(declared.name)
                .expect("a declared connector resolves");
            assert_eq!(resolved.name(), declared.name);
            assert_eq!(resolved.supply(), declared.supply());
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
        for declared in DECLARED {
            let expected = if declared.name == "static" {
                Supply::Inline
            } else {
                Supply::OnDemand
            };
            assert_eq!(declared.supply(), expected, "{}", declared.name);
        }
    }

    /// Every declared exchange is reachable, and each names a distinct endpoint.
    ///
    /// The property that has to survive the next connector: three providers
    /// share ONE refresh implementation and differ only by URL, so a copied row
    /// that forgot to change the endpoint would silently mint against the
    /// wrong vendor. Nothing else in the table can catch that.
    #[test]
    fn no_two_connectors_share_a_token_endpoint() {
        let endpoints: Vec<&str> = DECLARED
            .iter()
            .filter_map(|declared| match declared.exchange() {
                Exchange::OAuthRefresh { token_url } => Some(token_url),
                Exchange::Stored | Exchange::GithubApp => None,
            })
            .collect();
        assert!(
            !endpoints.is_empty(),
            "the refresh exchange has no declared user, so nothing proves it"
        );
        for (index, endpoint) in endpoints.iter().enumerate() {
            assert!(
                !endpoints
                    .iter()
                    .skip(index + 1)
                    .any(|other| other == endpoint),
                "`{endpoint}` is declared twice: a copied row kept its neighbour's vendor"
            );
            assert!(
                endpoint.starts_with("https://"),
                "`{endpoint}` posts a refresh token in the clear"
            );
        }
    }
}
