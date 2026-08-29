//! What a connection URL's `sslmode` resolves to, and what the boot line says.
//!
//! Split from `tests/config.rs` because it is a different question with a
//! different failure: that file proves the env surface a deployment already
//! has, this one proves a security posture. Both run against [`MapEnv`] for
//! the same reason — `std::env::set_var` is `unsafe` in edition 2024, and a
//! parallel suite racing on the process environment is undefined behaviour.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::env::MapEnv;
use afd_db::config::{DbRole, PoolConfig};

const URL: &str = "postgres://agentsfleet:secret@localhost:5432/agentsfleetdb?sslmode=disable";

fn env_with(pairs: &[(&str, &str)]) -> MapEnv {
    MapEnv::from_pairs(pairs.iter().copied())
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

    // The negative half, and every row agrees with sqlx rather than with an
    // idea of what the operator meant.
    //
    // `not-sslmode` matters because a key that merely CONTAINS a spelling must
    // not count, or it would suppress the upgrade. `SSLMODE` matters more, and
    // is the uncomfortable one: sqlx compares the key case-sensitively and
    // answers an upper-case spelling by logging `ignoring unrecognized connect
    // parameter` and moving on. So an operator who writes it gets `require`
    // and a boot failure against a server with no TLS. That is not this
    // branch's doing — it read the same way before — and the alternative is
    // worse: honouring a key sqlx ignores would skip the upgrade over a
    // `disable` that never reached the driver, which is a silent cleartext
    // connection instead of a loud refusal. Pinned so the agreement is a
    // decision rather than an accident.
    for ignored in ["not-sslmode", "SSLMODE", "SslMode"] {
        let url = format!("postgres://u:pw@h:5432/db?{ignored}=disable");
        let env = env_with(&[("DATABASE_URL", url.as_str())]);
        let config = PoolConfig::resolve(&env, DbRole::Default).unwrap();
        assert_eq!(
            config.ssl_mode(),
            "require",
            "sqlx ignores `{ignored}`, so it cannot count as a declaration"
        );
    }
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

/// Every mode a connection URL can declare reaches the log under its own name.
///
/// The three ordinary ones are covered by the tests above; these are the rest,
/// and they are here because the spelling is a contract with whoever is reading
/// the boot line at three in the morning. A mode that renders as the wrong word
/// — or as a fallback word covering several — sends them to check a posture the
/// daemon is not in, and nothing about the log looks wrong.
#[test]
fn test_every_declarable_mode_reports_under_its_own_name() {
    for mode in [
        "disable",
        "allow",
        "prefer",
        "require",
        "verify-ca",
        "verify-full",
    ] {
        let url = format!("postgres://u:pw@h:5432/db?sslmode={mode}");
        let env = env_with(&[("DATABASE_URL", url.as_str())]);
        let config = PoolConfig::resolve(&env, DbRole::Default)
            .unwrap_or_else(|error| panic!("{url} was refused: {error}"));
        assert_eq!(config.ssl_mode(), mode, "{url} reported the wrong mode");
    }
}
