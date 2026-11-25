//! What a fleet's toggles and approved grants become on the wire.

use std::borrow::Cow;

use af_fleet_runtime::config::{HttpEgress as Stored, RepositoryBinding as Approved};
use af_wire::policy::{HttpEgress, RepositoryAccess};
use serde_json::{Map, Value, json};

use super::{Network, network, wire_binding};

type Outcome = Result<(), Box<dyn std::error::Error>>;

fn statics(pairs: Value) -> Map<String, Value> {
    match pairs {
        Value::Object(map) => map,
        _other => Map::new(),
    }
}

fn allow<'a>(policy: &'a af_wire::policy::NetworkPolicy<'_>) -> Vec<&'a str> {
    policy.allow.iter().map(Cow::as_ref).collect()
}

#[test]
fn a_fleet_with_nothing_approved_reaches_no_named_host() {
    let policy = network(Network::default(), &Map::new(), false);

    assert_eq!(allow(&policy), [] as [&str; 0]);
    assert_eq!(policy.http_egress, HttpEgress::Restricted);
    assert!(!policy.package_manager_domains);
}

#[test]
fn each_approved_secret_adds_its_own_host_and_github_adds_the_api() {
    let held = statics(json!({
        "sentry": {"token": "s", "host": "sentry.io"},
        "grafana": {"token": "g", "host": "grafana.example.com"},
    }));
    let policy = network(Network::default(), &held, true);

    assert_eq!(
        allow(&policy),
        ["sentry.io", "grafana.example.com", "api.github.com"]
    );
}

#[test]
fn a_secret_without_a_readable_host_adds_nothing() {
    let held = statics(json!({
        "bare": {"token": "b"},
        "numeric": {"token": "n", "host": 443},
        "null": {"token": "z", "host": null},
    }));
    assert_eq!(
        allow(&network(Network::default(), &held, false)),
        [] as [&str; 0]
    );
}

#[test]
fn two_secrets_on_one_host_name_it_once() {
    let held = statics(json!({
        "pushover": {"token": "p", "host": "api.pushover.net"},
        "pushover-team": {"token": "q", "host": "API.Pushover.net"},
    }));
    assert_eq!(
        allow(&network(Network::default(), &held, false)),
        ["api.pushover.net"]
    );
}

#[test]
fn a_person_opens_the_network_with_the_two_toggles() {
    let policy = network(
        Network {
            http_egress: Stored::Public,
            package_domains: true,
        },
        &Map::new(),
        false,
    );

    assert_eq!(policy.http_egress, HttpEgress::Public);
    assert!(policy.package_manager_domains);
}

#[test]
fn an_approved_write_scope_reaches_the_run_unchanged() -> Outcome {
    let approved = Approved::parse(
        r#"{"repositories":["acme/payments","acme/ledger"],"access":"write","base":"main"}"#,
    )?;
    let carried = wire_binding(&approved);

    assert_eq!(carried.repositories, vec!["acme/payments", "acme/ledger"]);
    assert_eq!(carried.access, RepositoryAccess::Write);
    assert_eq!(carried.base_branch, "main");
    Ok(())
}

#[test]
fn a_read_scope_names_no_base_and_says_so_as_empty() -> Outcome {
    // The wire field is not optional, so "none" has exactly one spelling and a
    // reader testing for it must not also have to test for null.
    let approved = Approved::parse(r#"{"repositories":["acme/widgets"],"access":"read"}"#)?;
    let carried = wire_binding(&approved);

    assert_eq!(carried.access, RepositoryAccess::Read);
    assert_eq!(carried.base_branch, "");
    Ok(())
}
