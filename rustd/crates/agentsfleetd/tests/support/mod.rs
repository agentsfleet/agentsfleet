//! Shared scaffolding for the daemon's suites.
//!
//! Several test binaries include this module, so each compiles its own copy and
//! uses a subset of it — what one suite never calls is not dead code, it is
//! another suite's half.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]

/// The identity provider knobs, well-formed and never dialled.
///
/// The daemon refuses to boot without a provider — `preflight` requires it the
/// way `runtime_validate.zig` does — so every suite that expects a boot to
/// SUCCEED, or that means to fault one specific knob, has to supply these.
/// Declared once here because five suites need the same four values, and a
/// second spelling of the issuer would mean a test asserting against a provider
/// no other test configures (RULE UFS).
///
/// They only have to be well-formed: preflight resolves settings rather than
/// connecting, and both identity seams resolve lazily, so nothing in any lane
/// dials them.
pub(crate) const IDENTITY: [(&str, &str); 4] = [
    ("OIDC_ISSUER", "https://identity.fixture.test"),
    ("OIDC_AUDIENCE", "agentsfleetd-lane"),
    ("CLERK_API_BASE", "https://api.identity.fixture.test"),
    (
        "CLERK_SECRET_KEY",
        "fixture-provider-secret-not-a-credential",
    ),
];

/// Every identity knob name, including the optional one.
///
/// For the suites that strip the inherited environment knob by knob: a
/// developer with a real `OIDC_ISSUER` exported would otherwise turn a
/// negative case green on their machine and nowhere else. `OIDC_JWKS_URL` is
/// here and absent from [`IDENTITY`] on purpose — it is optional, derived from
/// the issuer, and still has to be cleared.
pub(crate) const IDENTITY_KNOBS: [&str; 5] = [
    "OIDC_ISSUER",
    "OIDC_AUDIENCE",
    "OIDC_JWKS_URL",
    "CLERK_API_BASE",
    "CLERK_SECRET_KEY",
];

/// Installs a subscriber so event macros actually run.
///
/// `tracing::error!` asks whether its callsite is enabled BEFORE evaluating the
/// fields inside it, so with no subscriber a diagnostic's fields never run —
/// the failure path executes and the line reporting it does not. Output goes to
/// a sink; the point is evaluation, not reading.
pub(crate) fn install_subscriber() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // A sink by default, stderr under `AFD_TEST_LOG`. The default is what
        // keeps a passing lane readable; the switch is what makes a live-service
        // failure diagnosable without editing this file, which is the shape the
        // §7 suite needs — the daemon's own refusal reason is the only account
        // of why a poll answered no-work.
        if std::env::var_os("AFD_TEST_LOG").is_some() {
            let subscriber = tracing_subscriber::fmt()
                .with_max_level(tracing::Level::TRACE)
                .with_writer(std::io::stderr)
                .finish();
            let _previous = tracing::subscriber::set_global_default(subscriber);
        } else {
            let subscriber = tracing_subscriber::fmt()
                .with_max_level(tracing::Level::TRACE)
                .with_writer(std::io::sink)
                .finish();
            let _previous = tracing::subscriber::set_global_default(subscriber);
        }
    });
}
