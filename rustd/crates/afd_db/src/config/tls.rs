//! The TLS posture a role's connection resolves to.
//!
//! Split from `config.rs` because it is one question with its own history and
//! its own tests: given a connection URL, does the operator say what to do
//! about transport security, and if not, what does this daemon do instead.
//! Everything else in `config` reads knobs; this decides a security posture.
//!
//! # TLS is required unless the URL says otherwise
//!
//! `sqlx` defaults to `sslmode=prefer` — encrypt if the server offers it,
//! continue in the clear if it does not. Every role-separated connection here
//! goes to a hosted provider that mandates TLS, so the default is `require`
//! and a URL that wants otherwise has to say `?sslmode=disable`, which is what
//! the local compose Postgres does.

use std::str::FromStr as _;

use sqlx::postgres::{PgConnectOptions, PgSslMode};
use url::Url;

use super::DbRole;
use crate::error::{Error, ErrorKind, Result};

/// The two spellings a Postgres URL may carry, and the only two.
const POSTGRES_SCHEMES: [&str; 2] = ["postgres://", "postgresql://"];

/// The spellings sqlx accepts for the SSL-mode connection parameter.
///
/// This pair is sqlx's, not ours — its URL parser matches `"sslmode" |
/// "ssl-mode"` and honours either. Asking a narrower set than the parser
/// answers IS the divergence that made the previous substring scan wrong, so
/// `test_declared_spellings_are_the_ones_sqlx_honours` grades this list against
/// the parser rather than against a reading of its source.
const SSLMODE_QUERY_KEYS: [&str; 2] = ["sslmode", "ssl-mode"];

/// The connection parameters sqlx reads as TLS certificate inputs, spelled
/// the ways its parser accepts them, canonical spelling first — the one an
/// error names regardless of the alias an operator wrote.
const SSL_ROOT_CERT_PARAM: &str = "sslrootcert";
const SSL_CERT_PARAM: &str = "sslcert";
const SSL_KEY_PARAM: &str = "sslkey";
const CERT_FILE_PARAMS: [(&str, &[&str]); 3] = [
    (
        SSL_ROOT_CERT_PARAM,
        &[SSL_ROOT_CERT_PARAM, "ssl-root-cert", "ssl-ca"],
    ),
    (SSL_CERT_PARAM, &[SSL_CERT_PARAM, "ssl-cert"]),
    (SSL_KEY_PARAM, &[SSL_KEY_PARAM, "ssl-key"]),
];

/// How sqlx tells inline PEM data from a file path (`From<String> for
/// CertificateInput`): trim, then both markers.
const PEM_MARKER_START: &str = "-----BEGIN";
const PEM_MARKER_END: &str = "-----";
const REDACTED_CERT_INPUT: &str = "<redacted certificate input>";

/// The boot event carrying one role's resolved TLS posture.
const SSL_MODE_RESOLVED_EVENT: &str = "db_ssl_mode_resolved";

/// Parses a connection URL, defaulting TLS to required.
///
/// The default is applied only when the URL is silent: `?sslmode=disable` is
/// how the local compose Postgres — which serves no TLS at all — is reachable,
/// and honouring it is why the local lane works without a certificate.
///
/// # Why "is it silent?" is asked of the parse and not of the string
///
/// It used to be asked of the string — split on the first `?`, then on `&`,
/// then compare key bytes — and a string scan and a URL parser do not agree on
/// what a query is. Two disagreements were reachable from a connection string
/// an operator can write, and each moved the TLS decision:
///
/// - `…/db#?sslmode=disable` puts the `?` inside the FRAGMENT. Nothing is
///   declared, so this should upgrade to `require`; the scan saw a query, read
///   a declaration, and left the connection on sqlx's `prefer` — which
///   continues in the clear against a server that offers no TLS.
/// - `?ssl-mode=disable` and `?ssl%6Dode=disable` are both honoured by sqlx,
///   whose parser accepts the alias and decodes the key. The scan compared raw
///   bytes against one spelling, read "undeclared", and forced `require` over
///   an operator's explicit `disable` — a boot failure whose message says
///   nothing about the knob that caused it.
pub(super) fn connect_options(role: DbRole, url: &str) -> Result<PgConnectOptions> {
    let knob = role.url_knob();

    // The scheme is checked here rather than left to sqlx, which accepts
    // `mysql://host/db` and reads it as host `host`, database `db`. A
    // deployment that pasted the wrong URL then connects somewhere real and
    // fails on the first query instead of at boot. `parseUrl` refuses anything
    // but these two prefixes (`pool.zig:81-87`) and so does this.
    if !POSTGRES_SCHEMES
        .iter()
        .any(|scheme| url.starts_with(scheme))
    {
        return Err(Error::new(ErrorKind::InvalidDatabaseUrlScheme { knob }));
    }

    let options = PgConnectOptions::from_str(url).map_err(|source| {
        Error::new(ErrorKind::InvalidDatabaseUrl {
            knob,
            source: Box::new(source),
        })
    })?;

    // `sqlx_core::Url` IS this crate, so the questions asked here and the
    // answers sqlx acted on come from one parse of one grammar — which is the
    // whole point, since a second grammar is a second set of inputs to
    // disagree on.
    //
    // Fallible rather than an `expect`, though a string sqlx already accepted
    // cannot fail here: the safe answer to "the query could not be read" is
    // the same as the answer to "the query said nothing", and trading a boot
    // that encrypts for a boot that panics is not an improvement.
    let parsed = Url::parse(url).ok();

    let declared = parsed.as_ref().is_some_and(declares_ssl_mode);
    let options = if declared {
        options
    } else {
        options.ssl_mode(PgSslMode::Require)
    };

    // Before anything dials: a certificate file sqlx will consume must be
    // readable here, where the failure can name the knob, parameter, and path.
    // Its connection path returns the plain socket immediately for `disable`
    // and `allow`, before constructing TLS or opening any certificate input.
    if !matches!(
        options.get_ssl_mode(),
        PgSslMode::Disable | PgSslMode::Allow
    ) && let Some(parsed) = parsed.as_ref()
    {
        reject_unreadable_cert_files(knob, parsed)?;
    }

    // Hoisted: see the `tracing` note in the workspace Cargo.toml. The URL
    // itself is never a field here — `knob` is the variable's NAME, and every
    // other value is decided rather than copied, so no userinfo can reach a log
    // sink through this line.
    let role_tag = role.tag();
    let ssl_mode = ssl_mode_tag(options.get_ssl_mode());
    tracing::info!(
        knob,
        role = role_tag,
        ssl_mode,
        declared,
        event = SSL_MODE_RESOLVED_EVENT
    );
    Ok(options)
}

/// Whether the URL's parsed query declares an SSL mode.
///
/// `query_pairs` percent-decodes the key, which is what makes this agree with
/// sqlx on `ssl%6Dode`, and reads only the query, which is what makes it agree
/// on a fragment that contains a `?`.
fn declares_ssl_mode(url: &Url) -> bool {
    url.query_pairs()
        .any(|(key, _mode)| SSLMODE_QUERY_KEYS.iter().any(|known| key == *known))
}

/// Fails a declared certificate file this process cannot read, naming the
/// knob, the parameter, and the path.
///
/// The inline-PEM carve-out mirrors sqlx's own classification: a value that
/// reads as PEM is data, not a path. sqlx's `PGSSLROOTCERT`-style environment
/// surface is deliberately not read — this daemon's knobs are the role URLs,
/// and a variable nothing here documents is not a knob to honour.
fn reject_unreadable_cert_files(knob: &'static str, url: &Url) -> Result<()> {
    for (canonical, aliases) in CERT_FILE_PARAMS {
        // Last declaration wins, matching sqlx's overwrite-on-repeat parse;
        // checking an earlier one could fail a file the driver never opens.
        let Some((_, value)) = url
            .query_pairs()
            .filter(|(key, _)| aliases.contains(&key.as_ref()))
            .last()
        else {
            continue;
        };

        let trimmed = value.trim();
        if trimmed.starts_with(PEM_MARKER_START) && trimmed.ends_with(PEM_MARKER_END) {
            continue;
        }

        std::fs::read(value.as_ref()).map_err(|source| {
            Error::new(ErrorKind::TlsCertFileUnreadable {
                knob,
                param: canonical,
                path: cert_input_for_error(value.as_ref()),
                source,
            })
        })?;
    }
    Ok(())
}

/// Names a path safely without copying malformed inline certificate material
/// or terminal control characters into the fatal error.
fn cert_input_for_error(value: &str) -> String {
    if value.trim().starts_with(PEM_MARKER_START) {
        REDACTED_CERT_INPUT.to_owned()
    } else {
        value.escape_debug().collect()
    }
}

/// The lower-case spelling of a resolved SSL mode.
///
/// The vocabulary is the connection URL's own, so an operator reading the boot
/// line and an operator reading the knob are reading one set of words. The
/// match is exhaustive on purpose: a variant sqlx adds must fail this build
/// rather than reach a log as a fallback word that means nothing.
pub(super) const fn ssl_mode_tag(mode: PgSslMode) -> &'static str {
    match mode {
        PgSslMode::Disable => "disable",
        PgSslMode::Allow => "allow",
        PgSslMode::Prefer => "prefer",
        PgSslMode::Require => "require",
        PgSslMode::VerifyCa => "verify-ca",
        PgSslMode::VerifyFull => "verify-full",
    }
}
