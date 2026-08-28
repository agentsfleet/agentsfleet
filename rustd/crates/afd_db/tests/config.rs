//! Dimension 2.4 (config half) — the env surface a deployment already has.
//!
//! Every knob name is the Zig daemon's, so these tests are the proof that a
//! deployment's existing environment means the same thing to both binaries.
//! They run against [`MapEnv`] rather than the process environment because
//! `std::env::set_var` is `unsafe` in edition 2024 — a parallel test suite
//! racing on the process environment is undefined behaviour, not flakiness.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_core::env::MapEnv;
use afd_db::config::{DbRole, EnvBool, PoolConfig, parse_env_bool};

const URL: &str = "postgres://agentsfleet:secret@localhost:5432/agentsfleetdb?sslmode=disable";

fn env_with(pairs: &[(&str, &str)]) -> MapEnv {
    MapEnv::from_pairs(pairs.iter().copied())
}

/// The three roles read the three knobs the daemon documents, and no others.
#[test]
fn test_role_url_knobs_match_the_zig_daemon() {
    assert_eq!(DbRole::Default.url_knob(), "DATABASE_URL");
    assert_eq!(DbRole::Api.url_knob(), "DATABASE_URL_API");
    assert_eq!(DbRole::Migrator.url_knob(), "DATABASE_URL_MIGRATOR");
    assert_eq!(DbRole::ALL.len(), 3, "a new role needs a knob and a tag");
}

/// A role resolves from its own knob and is not quietly served another's.
///
/// The failure this prevents is the expensive one: an API pool that silently
/// falls back to `DATABASE_URL` runs request-path queries with the migrator's
/// privileges, and nothing about that looks wrong until it does.
#[test]
fn test_each_role_resolves_only_its_own_knob() {
    let env = env_with(&[("DATABASE_URL", URL)]);
    PoolConfig::resolve(&env, DbRole::Default).expect("DATABASE_URL resolves");

    for role in [DbRole::Api, DbRole::Migrator] {
        let error = PoolConfig::resolve(&env, role).expect_err("no fallback between roles");
        assert!(error.is_config(), "{role:?} gave {error}");
        assert!(
            error.to_string().contains(role.url_knob()),
            "the failure must name the knob to set: {error}"
        );
    }
}

/// Unset and blank are the same thing, because a deployment that exports an
/// empty variable has not configured a database.
#[test]
fn test_blank_url_is_refused_like_an_unset_one() {
    for value in ["", "   ", "\t\n"] {
        let env = env_with(&[("DATABASE_URL", value)]);
        let error = PoolConfig::resolve(&env, DbRole::Default).expect_err("blank is not a URL");
        assert!(error.is_config(), "{value:?} gave {error}");
        assert_eq!(error.code().as_str(), "UZ-INTERNAL-001");
    }
}

/// A URL that is not a Postgres URL is refused at resolve, not at first query.
#[test]
fn test_malformed_url_is_refused_at_resolve() {
    for bad in ["mysql://host/db", "not-a-url", "://"] {
        let env = env_with(&[("DATABASE_URL", bad)]);
        let error = PoolConfig::resolve(&env, DbRole::Default).expect_err("not a Postgres URL");
        assert!(error.is_config(), "{bad:?} gave {error}");
    }
}

/// Both spellings of the scheme are accepted, as `parseUrl` accepts both.
#[test]
fn test_both_url_schemes_are_accepted() {
    for url in [
        "postgres://user:pw@host:5432/db",
        "postgresql://user:pw@host:5432/db",
    ] {
        let env = env_with(&[("DATABASE_URL", url)]);
        PoolConfig::resolve(&env, DbRole::Default)
            .unwrap_or_else(|error| panic!("{url} was refused: {error}"));
    }
}

/// Defaults, when nothing is tuned: four connections, a two-second acquire.
#[test]
fn test_defaults_match_the_documented_sizing() {
    let env = env_with(&[("DATABASE_URL", URL)]);
    let config = PoolConfig::resolve(&env, DbRole::Default).unwrap();
    assert_eq!(config.max_connections(), 4, "256 in-flight / 64 per conn");
    assert_eq!(config.acquire_timeout(), Duration::from_millis(2_000));
    assert_eq!(config.connect_timeout(), Duration::from_millis(10_000));
}

/// A role-scoped override beats the shared knob; the shared knob applies to
/// every role that does not override it.
#[test]
fn test_role_scoped_knob_beats_the_shared_one() {
    let env = env_with(&[
        ("DATABASE_URL", URL),
        ("DATABASE_URL_API", URL),
        ("DATABASE_POOL_SIZE", "12"),
        ("DATABASE_POOL_SIZE_API", "40"),
        ("DATABASE_ACQUIRE_TIMEOUT_MS", "500"),
    ]);

    let shared = PoolConfig::resolve(&env, DbRole::Default).unwrap();
    assert_eq!(shared.max_connections(), 12);
    assert_eq!(shared.acquire_timeout(), Duration::from_millis(500));

    let scoped = PoolConfig::resolve(&env, DbRole::Api).unwrap();
    assert_eq!(scoped.max_connections(), 40, "_API must win");
    assert_eq!(
        scoped.acquire_timeout(),
        Duration::from_millis(500),
        "an unscoped knob still applies to the scoped role"
    );
}

/// A pool size that would hang the daemon falls back to the default instead.
///
/// Zero is the one that matters: a zero-connection pool accepts every acquire
/// and satisfies none, so the wrong knob reads as a datastore outage.
#[test]
fn test_unusable_pool_sizes_fall_back_to_the_default() {
    for value in ["0", "banana", "", "99999999999999999999"] {
        let env = env_with(&[("DATABASE_URL", URL), ("DATABASE_POOL_SIZE", value)]);
        let config = PoolConfig::resolve(&env, DbRole::Default).unwrap();
        assert_eq!(config.max_connections(), 4, "{value:?} did not fall back");
    }
}

/// Surrounding whitespace is trimmed, because exported values pick it up.
#[test]
fn test_knob_values_are_trimmed() {
    let env = env_with(&[
        ("DATABASE_URL", "  postgres://u:p@host:5432/db  "),
        ("DATABASE_POOL_SIZE", " 7\n"),
    ]);
    let config = PoolConfig::resolve(&env, DbRole::Default).unwrap();
    assert_eq!(config.max_connections(), 7);
}

/// The boolean grammar every knob in this product shares, including the third
/// answer: a value that is neither is a misconfiguration, not a false.
#[test]
fn test_env_bool_grammar_has_three_answers() {
    for yes in ["true", "TRUE", "True", "1", " true "] {
        assert_eq!(parse_env_bool(yes), EnvBool::Yes, "{yes:?}");
    }
    for no in ["false", "FALSE", "0", " 0\t"] {
        assert_eq!(parse_env_bool(no), EnvBool::No, "{no:?}");
    }
    for invalid in ["maybe", "yes", "on", "", "2"] {
        assert_eq!(parse_env_bool(invalid), EnvBool::Invalid, "{invalid:?}");
    }
}

/// `MIGRATE_ON_START` is off when unset and refuses a value it cannot read.
///
/// `yes` is the case worth naming: an operator who wrote it believes
/// migrations are on. Reading it as `false` would leave a deployment silently
/// un-migrated, which is why `cmd/common.zig` makes it a boot error.
#[test]
fn test_migrate_on_start_refuses_what_it_cannot_read() {
    assert!(!afd_db::migrate_on_start(&env_with(&[])).unwrap());
    assert!(afd_db::migrate_on_start(&env_with(&[("MIGRATE_ON_START", "1")])).unwrap());
    assert!(!afd_db::migrate_on_start(&env_with(&[("MIGRATE_ON_START", "0")])).unwrap());

    let error = afd_db::migrate_on_start(&env_with(&[("MIGRATE_ON_START", "yes")]))
        .expect_err("an unreadable value is not a false");
    assert!(error.is_config(), "got {error}");
    assert!(error.to_string().contains("MIGRATE_ON_START"));
}

/// A URL with the right scheme that sqlx still cannot parse is refused, and the
/// failure says *that* rather than blaming the scheme.
///
/// The two are different operator instructions: a wrong scheme means the URL
/// points at another product, while an unparseable one means this URL is
/// malformed. Reporting the scheme message for a bad port sends whoever is
/// holding the pager looking for a MySQL URL that was never there.
#[test]
fn test_a_postgres_url_sqlx_cannot_parse_is_refused_as_malformed() {
    for bad in [
        "postgres://user:pw@host:99999/db",
        "postgres://user:pw@[::1/db",
        "postgres://user:pw@host:notanumber/db",
        "postgresql://user:pw@host/db?sslmode=banana",
    ] {
        let env = env_with(&[("DATABASE_URL", bad)]);
        let error = PoolConfig::resolve(&env, DbRole::Default).expect_err("unparseable");
        assert!(error.is_config(), "{bad:?} gave {error}");
        let rendered = error.to_string();
        assert!(
            rendered.contains("is not a Postgres connection URL"),
            "{bad:?} was refused as a scheme problem instead: {rendered}"
        );
        assert!(
            rendered.contains("DATABASE_URL"),
            "the failure must name the knob to fix: {rendered}"
        );
    }
}

/// The scheme check and the parse check stay distinct.
#[test]
fn test_a_wrong_scheme_is_not_reported_as_a_malformed_url() {
    let env = env_with(&[("DATABASE_URL", "mysql://user:pw@host:3306/db")]);
    let error = PoolConfig::resolve(&env, DbRole::Default).expect_err("wrong scheme");
    assert!(
        error
            .to_string()
            .contains("must be a postgres:// or postgresql:// URL"),
        "got {error}"
    );
}
