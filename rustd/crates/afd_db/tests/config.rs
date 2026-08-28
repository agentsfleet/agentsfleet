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

/// The inputs on which the connection string's `sslmode` is decided.
///
/// Committed green against the substring scan that used to decide this, then
/// turned over by the commit that replaced it — the three `was ...` rows are
/// that diff, kept here because the behaviour they describe is the reason the
/// scan went. A reader who deletes the parse and restores the scan gets those
/// three rows back as failures, which is the only durable proof that the two
/// implementations were ever different.
#[test]
fn test_sslmode_detection_pinned_against_hand_rolled() {
    assert!(
        std::env::var_os("PGSSLMODE").is_none(),
        "PGSSLMODE is set here; sqlx reads it as the default for a URL that \
         declares nothing, so these rows would grade against the environment \
         rather than against the daemon's own default"
    );

    // (url, resolved mode, what the row is for)
    let pinned = [
        (
            "postgres://u:pw@h:5432/db",
            "require",
            "silent: the documented default",
        ),
        (
            "postgres://u:pw@h:5432/db?sslmode=disable",
            "disable",
            "declared plainly: honoured, and the local compose lane needs it",
        ),
        (
            "postgres://u:pw@h:5432/db?ssl-mode=disable",
            "disable",
            "was `require`: sqlx honours the alias, so an explicit disable is \
             no longer overridden into a boot failure",
        ),
        (
            "postgres://u:pw@h:5432/db?ssl%6Dode=disable",
            "disable",
            "was `require`: the key is decoded before it is compared, as the \
             parser decodes it",
        ),
        (
            "postgres://u:pw@h:5432/db#?sslmode=disable",
            "require",
            "was `prefer`, and this is the row that cost TLS: the `?` is in \
             the fragment, so nothing is declared and the upgrade applies",
        ),
    ];

    for (url, expected, why) in pinned {
        let env = env_with(&[("DATABASE_URL", url)]);
        let config = PoolConfig::resolve(&env, DbRole::Default)
            .unwrap_or_else(|error| panic!("{url} was refused: {error}"));
        assert_eq!(config.ssl_mode(), expected, "{url}\n  {why}");
    }
}

/// A password carrying the query syntax cannot smuggle a declaration past the
/// TLS default, in either of the two forms it can reach the daemon in.
///
/// The literal form is not a URL at all: `postgres://u:p?sslmode=disable@h/db`
/// ends its authority at the `?`, leaving `p` where a port belongs, and sqlx
/// refuses it before any of this crate's logic runs. The reachable form is the
/// percent-encoded one an operator's tooling actually produces, and there the
/// `?` is data inside the userinfo rather than a query delimiter — so nothing
/// is declared, and the connection requires TLS.
#[test]
fn test_password_bearing_query_syntax_still_requires_tls() {
    for url in [
        // A password of `p?sslmode=disable`, encoded as a URL requires.
        "postgres://u:p%3Fsslmode%3Ddisable@h:5432/db",
        // And with a `#` in it too, so neither delimiter is reachable raw.
        "postgres://u:p%23%3Fsslmode%3Ddisable@h:5432/db",
    ] {
        let env = env_with(&[("DATABASE_URL", url)]);
        let config = PoolConfig::resolve(&env, DbRole::Default)
            .unwrap_or_else(|error| panic!("{url} was refused: {error}"));
        assert_eq!(
            config.ssl_mode(),
            "require",
            "a password is not a query: {url}"
        );
    }

    let literal = "postgres://u:p?sslmode=disable@h/db";
    let env = env_with(&[("DATABASE_URL", literal)]);
    let error = PoolConfig::resolve(&env, DbRole::Default)
        .expect_err("an unencoded `?` in a password is not a parseable URL");
    assert!(
        error
            .to_string()
            .contains("is not a Postgres connection URL"),
        "got {error}"
    );
}

/// A percent-encoded `sslmode` key is honoured, because the parser decodes it.
///
/// The direction of this failure was the quiet one: the old scan compared the
/// raw key, read "undeclared", and forced `require` over an operator who had
/// written `disable` — so a deployment against a Postgres serving no TLS
/// failed to boot, and the message named neither the knob nor the reason.
#[test]
fn test_percent_encoded_sslmode_key_is_honoured() {
    for url in [
        "postgres://u:pw@h:5432/db?ssl%6Dode=disable",
        "postgres://u:pw@h:5432/db?%73slmode=disable",
    ] {
        let env = env_with(&[("DATABASE_URL", url)]);
        let config = PoolConfig::resolve(&env, DbRole::Default)
            .unwrap_or_else(|error| panic!("{url} was refused: {error}"));
        assert_eq!(
            config.ssl_mode(),
            "disable",
            "the operator's declared mode must win: {url}"
        );
    }
}

/// The spellings this crate treats as a declaration are the ones sqlx acts on.
///
/// This is the test that keeps the pair honest rather than documented. Each
/// spelling is fed to sqlx as an explicit `disable`; if sqlx stops honouring
/// one, that spelling stops reaching `disable` here and this fails — which is
/// the failure worth having, because a spelling sqlx honours and this crate
/// does not is a forced `require`, and one this crate honours and sqlx does not
/// is a skipped upgrade.
#[test]
fn test_declared_spellings_are_the_ones_sqlx_honours() {
    for key in ["sslmode", "ssl-mode"] {
        let url = format!("postgres://u:pw@h:5432/db?{key}=disable");
        let env = env_with(&[("DATABASE_URL", url.as_str())]);
        let config = PoolConfig::resolve(&env, DbRole::Default)
            .unwrap_or_else(|error| panic!("{url} was refused: {error}"));
        assert_eq!(
            config.ssl_mode(),
            "disable",
            "sqlx honours `{key}` and this crate must read it as a declaration"
        );
    }

    // The negative half: a key that merely contains one of the spellings is
    // not one, or `?not-sslmode=x` would suppress the upgrade.
    let url = "postgres://u:pw@h:5432/db?not-sslmode=disable";
    let env = env_with(&[("DATABASE_URL", url)]);
    let config = PoolConfig::resolve(&env, DbRole::Default).unwrap();
    assert_eq!(
        config.ssl_mode(),
        "require",
        "substring is not a declaration"
    );
}

/// Where a captured event's bytes go, or `None` when nothing is capturing.
static SINK: std::sync::Mutex<Option<Vec<u8>>> = std::sync::Mutex::new(None);

/// Serializes the capturing tests, since they share one sink.
static CAPTURE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The subscriber's writer: appends to [`SINK`] when one is armed, discards
/// otherwise.
struct SinkWriter;

/// The sink, poisoned or not.
///
/// A test that panicked while holding it already failed; making the NEXT test
/// fail for that reason would report one defect twice and hide the second.
fn sink() -> std::sync::MutexGuard<'static, Option<Vec<u8>>> {
    SINK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl std::io::Write for SinkWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Some(captured) = sink().as_mut() {
            captured.extend_from_slice(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Runs `emit` and returns what it logged.
///
/// # Why the subscriber is global and installed once
///
/// A scoped `with_default` looks like the tidier choice and does not work here.
/// `tracing` caches an `Interest` per CALLSITE, and every other test in this
/// file resolves a `PoolConfig`, which reaches the same `tracing::info!` — so
/// whichever of them runs first with no subscriber installed caches that
/// callsite as never-interested, and a scoped subscriber installed afterwards
/// reads an empty buffer. The test then asserts over an empty string and passes
/// vacuously, which is RULE TCF's exact failure: it was found by the ONE
/// assertion here that happens to be a `contains`, not by the suite going red.
///
/// One global subscriber at `TRACE` keeps the callsite permanently interested;
/// the writer, not the filter, decides whether the bytes are kept.
fn captured_events(emit: impl FnOnce()) -> String {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(|| SinkWriter)
            .finish();
        tracing::subscriber::set_global_default(subscriber)
            .expect("this binary installs no other global subscriber");
    });

    let _serialized = CAPTURE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    *sink() = Some(Vec::new());
    emit();
    let captured = sink().take().unwrap_or_default();

    String::from_utf8(captured).expect("tracing writes utf-8")
}

/// The resolved mode reaches the operator's log, and the connection string
/// does not.
///
/// Both halves matter. Without the line, an operator whose deployment now
/// requires TLS where it used to prefer it has no way to see that from the
/// outside. With the wrong fields, the line is worse than absent: a connection
/// URL carries the password in its userinfo, and a log sink is the one place a
/// credential must never be copied to.
#[test]
fn test_resolved_ssl_mode_is_logged_without_the_url() {
    const PASSWORD: &str = "hunter2correcthorse";
    let url = format!("postgres://operator:{PASSWORD}@db.example.internal:5432/agentsfleetdb");

    let rendered = captured_events(|| {
        let env = env_with(&[("DATABASE_URL_API", url.as_str())]);
        let config = PoolConfig::resolve(&env, DbRole::Api).expect("resolves");
        assert_eq!(config.ssl_mode(), "require");
    });

    assert!(
        rendered.contains("event=\"db_ssl_mode_resolved\""),
        "the boot line must carry its event name: {rendered}"
    );
    for field in [
        "knob=\"DATABASE_URL_API\"",
        "role=\"api\"",
        "ssl_mode=\"require\"",
        "declared=false",
    ] {
        assert!(
            rendered.contains(field),
            "the boot line must carry {field}: {rendered}"
        );
    }

    for secret in [PASSWORD, "operator", "db.example.internal", "agentsfleetdb"] {
        assert!(
            !rendered.contains(secret),
            "{secret:?} reached a log sink: {rendered}"
        );
    }
}

/// A URL that declares its mode says so, so the two cases are distinguishable
/// in the log rather than only in the resolved value.
#[test]
fn test_the_boot_line_reports_whether_the_url_declared_the_mode() {
    let rendered = captured_events(|| {
        let env = env_with(&[("DATABASE_URL", URL)]);
        PoolConfig::resolve(&env, DbRole::Default).expect("resolves");
    });

    assert!(
        rendered.contains("ssl_mode=\"disable\"") && rendered.contains("declared=true"),
        "a declared mode must read as declared: {rendered}"
    );
}
