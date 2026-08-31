//! A declared TLS certificate file must fail at resolve — named.
//!
//! sqlx opens `sslrootcert`/`sslcert`/`sslkey` files deep in the TLS
//! handshake, so a missing one surfaced as `error communicating with database:
//! No such file or directory` — no knob, no parameter, no path, and only after
//! a TCP connection had already answered. These tests pin the resolve-time
//! refusal that replaced that chain. A sibling of `tests/config_tls.rs`
//! because that file is at the length cap.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::error::Error as _;

use afd_core::env::MapEnv;
use afd_db::config::{DbRole, PoolConfig};

const BASE: &str = "postgres://agentsfleet:secret@localhost:5432/agentsfleetdb";

fn env_with(pairs: &[(&str, &str)]) -> MapEnv {
    MapEnv::from_pairs(pairs.iter().copied())
}

fn migrator_env(url: &str) -> MapEnv {
    env_with(&[("DATABASE_URL_MIGRATOR", url)])
}

/// A path nothing creates, unique to this process so a stale file from an
/// earlier run cannot let a "missing" row pass vacuously.
fn missing_path() -> String {
    let path = std::env::temp_dir().join(format!(
        "agentsfleetd-no-such-cert-{}.pem",
        std::process::id()
    ));
    assert!(
        !path.exists(),
        "test premise broken: {} exists",
        path.display()
    );
    path.to_string_lossy().into_owned()
}

/// A real file for the rows that must RESOLVE, removed when the guard drops.
struct TempCert(std::path::PathBuf);

impl TempCert {
    fn create(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "agentsfleetd-cert-{}-{name}.pem",
            std::process::id()
        ));
        std::fs::write(&path, b"not a real cert; resolve only reads it").unwrap();
        Self(path)
    }

    fn as_str(&self) -> &str {
        self.0.to_str().expect("temp path is utf-8")
    }
}

impl Drop for TempCert {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

struct TempDir(std::path::PathBuf);

impl TempDir {
    fn create(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "agentsfleetd-cert-dir-{}-{name}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn as_str(&self) -> &str {
        self.0.to_str().expect("temp path is utf-8")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir(&self.0);
    }
}

/// The failure this module exists for: the operator learns the knob, the
/// parameter, and the path, and the io error stays on the chain as the cause.
#[test]
fn test_a_missing_cert_file_fails_at_resolve_naming_knob_param_and_path() {
    let path = missing_path();
    let url = format!("{BASE}?sslmode=verify-full&sslrootcert={path}");
    let error = PoolConfig::resolve(&migrator_env(&url), DbRole::Migrator).unwrap_err();

    assert!(
        error.is_config(),
        "a bad cert file is a config failure, not an outage: {error}"
    );
    let shown = error.to_string();
    assert!(
        shown.contains("DATABASE_URL_MIGRATOR"),
        "knob named: {shown}"
    );
    assert!(shown.contains("sslrootcert"), "parameter named: {shown}");
    assert!(shown.contains(&path), "path named: {shown}");

    let source = error.source().expect("the io error is the cause");
    let io = source
        .downcast_ref::<std::io::Error>()
        .expect("the cause stays an io::Error");
    assert_eq!(io.kind(), std::io::ErrorKind::NotFound);
}

/// Every certificate-file spelling sqlx accepts is checked, while the error
/// names the canonical spelling an operator can look up.
#[test]
fn test_every_sqlx_cert_parameter_spelling_is_checked_and_named_canonically() {
    for (canonical, alias) in [
        ("sslrootcert", "sslrootcert"),
        ("sslrootcert", "ssl-root-cert"),
        ("sslrootcert", "ssl-ca"),
        ("sslcert", "sslcert"),
        ("sslcert", "ssl-cert"),
        ("sslkey", "sslkey"),
        ("sslkey", "ssl-key"),
    ] {
        let url = format!("{BASE}?sslmode=verify-full&{alias}={}", missing_path());
        let error = PoolConfig::resolve(&migrator_env(&url), DbRole::Migrator).unwrap_err();
        assert!(
            error.to_string().contains(canonical),
            "{canonical} named: {error}"
        );
    }
}

/// sqlx's own classification: a value reading as PEM is data, not a path, so
/// no file is looked for. Percent-decoded, the way `query_pairs` hands it over.
#[test]
fn test_inline_pem_is_data_not_a_path() {
    let url = format!(
        "{BASE}?sslmode=verify-full&sslrootcert=-----BEGIN%20CERTIFICATE-----Zm9v-----END%20CERTIFICATE-----"
    );
    PoolConfig::resolve(&migrator_env(&url), DbRole::Migrator)
        .expect("inline PEM is not a file to find");
}

#[test]
fn test_inline_pem_requires_both_markers() {
    for (value, expected_display) in [
        ("-----BEGIN-not-pem", "<redacted certificate input>"),
        ("not-pem-----", "not-pem-----"),
    ] {
        let url = format!("{BASE}?sslmode=verify-full&sslrootcert={value}");
        let error = PoolConfig::resolve(&migrator_env(&url), DbRole::Migrator).unwrap_err();
        assert!(
            error.to_string().contains(expected_display),
            "failed input identified safely: {error}"
        );
    }
}

#[test]
fn test_malformed_inline_certificate_material_is_not_echoed_as_a_path() {
    let url =
        format!("{BASE}?sslmode=verify-full&sslkey=-----BEGIN%20PRIVATE%20KEY-----super-secret");
    let error = PoolConfig::resolve(&migrator_env(&url), DbRole::Migrator).unwrap_err();
    let shown = error.to_string();

    assert!(shown.contains("sslkey"), "parameter named: {shown}");
    assert!(
        shown.contains("<redacted certificate input>"),
        "malformed inline input is identified without copying it: {shown}"
    );
    assert!(
        !shown.contains("super-secret"),
        "certificate material reached the fatal error: {shown}"
    );
}

#[test]
fn test_control_characters_in_a_cert_path_are_escaped() {
    let url = format!("{BASE}?sslmode=verify-full&sslrootcert=bad%0Apath");
    let error = PoolConfig::resolve(&migrator_env(&url), DbRole::Migrator).unwrap_err();
    let shown = error.to_string();

    assert!(shown.contains(r"bad\npath"), "escaped path named: {shown}");
    assert!(
        !shown.contains("bad\npath"),
        "raw newline reached the error"
    );
}

#[test]
fn test_a_cert_file_that_exists_resolves() {
    let cert = TempCert::create("present");
    let url = format!("{BASE}?sslmode=verify-full&sslrootcert={}", cert.as_str());
    PoolConfig::resolve(&migrator_env(&url), DbRole::Migrator).expect("a readable file resolves");
}

#[test]
fn test_a_directory_is_not_accepted_as_a_readable_cert_file() {
    let directory = TempDir::create("not-a-file");
    let url = format!(
        "{BASE}?sslmode=verify-full&sslrootcert={}",
        directory.as_str()
    );
    let error = PoolConfig::resolve(&migrator_env(&url), DbRole::Migrator).unwrap_err();

    let source = error.source().expect("the io error is the cause");
    let io = source
        .downcast_ref::<std::io::Error>()
        .expect("the cause stays an io::Error");
    assert_eq!(io.kind(), std::io::ErrorKind::IsADirectory);
}

/// sqlx overwrites on a repeated parameter, so only the last value is one the
/// driver would open; failing the first would refuse a file nobody reads.
#[test]
fn test_the_last_declaration_wins_like_sqlxs_parse() {
    let cert = TempCert::create("last-wins");
    let url = format!(
        "{BASE}?sslrootcert={}&sslrootcert={}",
        missing_path(),
        cert.as_str()
    );
    PoolConfig::resolve(&migrator_env(&url), DbRole::Migrator)
        .expect("the later, readable declaration is the one sqlx would use");
}
