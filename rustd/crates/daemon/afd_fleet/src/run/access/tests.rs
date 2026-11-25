//! Which approved GitHub scope covers a request, and which secret hosts a
//! request may name, decided without a datastore.
#![expect(
    clippy::expect_used,
    reason = "a refused fixture fails the proof loudly"
)]

use af_fleet_runtime::config::RepositoryBinding;
use serde_json::json;

use super::{covers, readable_host};

fn scope(value: &serde_json::Value) -> RepositoryBinding {
    RepositoryBinding::parse(&value.to_string()).expect("a scope the parser reads")
}

fn read(repositories: &[&str]) -> RepositoryBinding {
    scope(&json!({"repositories": repositories, "access": "read"}))
}

fn write(repositories: &[&str], base: &str) -> RepositoryBinding {
    scope(&json!({"repositories": repositories, "access": "write", "base": base}))
}

#[test]
fn an_approved_scope_covers_a_request_inside_it() {
    let held = write(&["org/app", "org/lib"], "main");

    assert!(covers(&held, &read(&["org/app"])));
    assert!(
        covers(&held, &read(&["ORG/App"])),
        "repositories match without case"
    );
    assert!(covers(&held, &write(&["org/lib"], "main")));
    assert!(covers(&read(&["org/app"]), &read(&["org/app"])));
}

#[test]
fn a_wider_request_is_not_covered() {
    let held = read(&["org/app"]);

    assert!(
        !covers(&held, &write(&["org/app"], "main")),
        "write past read"
    );
    assert!(
        !covers(&held, &read(&["org/app", "org/other"])),
        "another repository"
    );
    assert!(
        !covers(
            &write(&["org/app"], "main"),
            &write(&["org/app"], "release")
        ),
        "another base"
    );
}

#[test]
fn a_secret_host_must_name_a_reachable_host() {
    for stored in [
        json!({"token": "t"}),
        json!({"token": "t", "host": "api.sentry.io"}),
        json!({"token": "t", "host": "https://API.Sentry.io:443/api"}),
    ] {
        assert!(readable_host(&stored).is_ok(), "{stored}");
    }
    for stored in [
        json!({"token": "t", "host": 443}),
        json!({"token": "t", "host": "https://"}),
        json!({"token": "t", "host": ""}),
    ] {
        let refused = readable_host(&stored).expect_err("no host a request reaches");
        assert_eq!(
            refused.code(),
            af_core::error_code::INVALID_REQUEST,
            "{stored}"
        );
    }
}
