//! Shared scaffolding for the suites that boot this daemon: `agentsfleetd`'s
//! own and `af_e2e`'s, which drive a runner against it.
//!
//! A `test-util` module (M-TEST-UTIL) rather than a `tests/support` file each
//! suite includes by path, so the two crates share one copy.

use crate::preflight::SESSION_CODE_PEPPER_KNOB;

/// The device-flow pepper, well-formed and never used as a key here.
///
/// Required at boot for the reason the master key is: without it the login
/// surface would store an unpeppered verification-code digest, so a queue
/// somebody can read becomes a queue somebody can log in from. Every suite that
/// expects a boot to SUCCEED, or that means to fault one specific OTHER knob,
/// has to supply it — so it is shaped as a chainable pair-array like
/// [`IDENTITY`] and chained the same way.
///
/// Not a credential: sixty-four hexadecimal characters that never leave these
/// test binaries. Preflight only asks that the knob carry something, since the
/// value is an HMAC key and its shape is the operator's business rather than
/// this daemon's. The knob NAME is borrowed from the library instead of
/// respelled, so the string exists once in the tree (RULE UFS).
pub const SESSION_PEPPER: [(&str, &str); 1] = [(
    SESSION_CODE_PEPPER_KNOB,
    "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
)];

/// The event bus seed, well-formed and never dialled by a preflight-only test.
pub const BUS: [(&str, &str); 1] = [(afd_bus::nats::URL_ENV, "nats://127.0.0.1:4222")];

/// The identity provider knobs, well-formed and never dialled.
///
/// The daemon refuses to boot without a provider — `preflight` requires it —
/// so every suite that expects a boot to SUCCEED, or that means to fault one
/// specific knob, has to supply these.
/// Declared once here because five suites need the same four values, and a
/// second spelling of the issuer would mean a test asserting against a provider
/// no other test configures (RULE UFS).
///
/// They only have to be well-formed: preflight resolves settings rather than
/// connecting, and both identity seams resolve lazily, so nothing in any lane
/// dials them.
pub const IDENTITY: [(&str, &str); 4] = [
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
pub const IDENTITY_KNOBS: [&str; 5] = [
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
pub fn install_subscriber() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    #[expect(
        clippy::disallowed_methods,
        reason = "a presence check: AFD_TEST_LOG set to any value, blank or non-UTF-8 included, turns logging on, and af_core::env would read either as unset"
    )]
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

/// One process-local native identity keeps every fixture on ephemeral, directly bound ports.
pub mod runner_stream {
    use std::sync::LazyLock;

    use crate::preflight::runner_listener::{LISTEN_ADDR, TLS_CERT_FILE, TLS_KEY_FILE};

    #[expect(
        clippy::expect_used,
        reason = "a generated test identity failing must fail its fixture"
    )]
    static IDENTITY: LazyLock<af_stream::test_util::IdentityFixture> = LazyLock::new(|| {
        af_stream::test_util::IdentityFixture::new(vec![
            "localhost".to_owned(),
            "127.0.0.1".to_owned(),
            "::1".to_owned(),
        ])
        .expect("the local issuer can sign a fixture leaf")
    });

    static PATHS: LazyLock<[String; 2]> = LazyLock::new(|| {
        [
            IDENTITY.chain().to_string_lossy().into_owned(),
            IDENTITY.key().to_string_lossy().into_owned(),
        ]
    });

    /// The runner listener's knobs: an ephemeral loopback port and the fixture
    /// identity's certificate and key files.
    #[must_use]
    pub fn environment() -> [(&'static str, &'static str); 3] {
        let [certificate, key] = &*PATHS;
        [
            (LISTEN_ADDR, "[::1]:0"),
            (TLS_CERT_FILE, certificate.as_str()),
            (TLS_KEY_FILE, key.as_str()),
        ]
    }

    /// What a client trusts to reach the fixture listener.
    #[must_use]
    pub fn trust() -> af_stream::tls::Trust {
        IDENTITY.trust()
    }

    /// The fixture issuer's root certificate, as a file a runner reads.
    #[must_use]
    pub fn root_file() -> std::path::PathBuf {
        IDENTITY.root()
    }
}

/// A collector that keeps what an OTLP `http/json` exporter posts it: the
/// fixture this daemon's export suite and the runner's telemetry suite read
/// back what left the process through.
pub mod collector {
    use std::sync::{Arc, Mutex, PoisonError};

    use axum::Router;
    use axum::extract::State;
    use axum::routing::post;

    /// What the collector saw: the path, and the body it was sent.
    pub type Received = Arc<Mutex<Vec<(String, String)>>>;

    /// The trace signal's path.
    pub const TRACES: &str = "/v1/traces";
    /// The metric signal's path.
    pub const METRICS: &str = "/v1/metrics";
    /// The log signal's path.
    pub const LOGS: &str = "/v1/logs";

    /// A collector on an ephemeral loopback port: its base URL, and what it
    /// has been posted so far.
    ///
    /// # Panics
    /// When no loopback port can be bound, which fails the fixture.
    #[expect(
        clippy::expect_used,
        reason = "a fixture that cannot bind must fail its suite"
    )]
    pub async fn collector() -> (String, Received) {
        let received: Received = Arc::new(Mutex::new(Vec::new()));
        let app = Router::new()
            .route(TRACES, post(accept))
            .route(METRICS, post(accept))
            .route(LOGS, post(accept))
            .with_state(Arc::clone(&received));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("an ephemeral port is available");
        let address = listener.local_addr().expect("the port is bound");
        tokio::spawn(async move {
            let _served = axum::serve(listener, app).await;
        });
        (format!("http://{address}"), received)
    }

    /// Records one delivery and accepts it.
    async fn accept(
        State(received): State<Received>,
        request: axum::extract::Request,
    ) -> &'static str {
        let path = request.uri().path().to_owned();
        let body = axum::body::to_bytes(request.into_body(), usize::MAX)
            .await
            .unwrap_or_default();
        received
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((path, String::from_utf8_lossy(&body).into_owned()));
        ""
    }
}
