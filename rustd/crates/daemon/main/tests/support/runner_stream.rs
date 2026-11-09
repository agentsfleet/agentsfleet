//! One process-local native identity keeps every fixture on ephemeral, directly bound ports.

use std::sync::LazyLock;

use agentsfleetd::preflight::runner_listener::{LISTEN_ADDR, TLS_CERT_FILE, TLS_KEY_FILE};

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

pub(crate) fn environment() -> [(&'static str, &'static str); 3] {
    let [certificate, key] = &*PATHS;
    [
        (LISTEN_ADDR, "[::1]:0"),
        (TLS_CERT_FILE, certificate.as_str()),
        (TLS_KEY_FILE, key.as_str()),
    ]
}

pub(crate) fn trust() -> af_stream::tls::Trust {
    IDENTITY.trust()
}

pub(crate) fn root_file() -> std::path::PathBuf {
    IDENTITY.root()
}
