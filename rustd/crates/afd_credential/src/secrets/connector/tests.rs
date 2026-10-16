//! The shipped registry's table, and how a stored handle resolves through it.

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
    // The name IS the descriptor's field, so what is
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
