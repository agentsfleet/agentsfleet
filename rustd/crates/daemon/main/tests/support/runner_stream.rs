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
    [
        (LISTEN_ADDR, "[::1]:0"),
        (TLS_CERT_FILE, PATHS[0].as_str()),
        (TLS_KEY_FILE, PATHS[1].as_str()),
    ]
}

pub(crate) fn trust() -> af_stream::tls::Trust {
    IDENTITY.trust()
}

pub(crate) fn root_file() -> std::path::PathBuf {
    IDENTITY.root()
}

pub(crate) async fn client(run: &crate::e2e::Scenario) -> &afr_supervisor::test_util::Session {
    run.native
        .get_or_init(|| async {
            let origin = format!(
                "https://localhost:{}",
                run.booted.runner_stream_address.port()
            );
            let root = root_file().to_string_lossy().into_owned();
            let env = af_core::env::MapEnv::from_pairs([
                (afr_supervisor::config::ENV_API_URL, run.base.as_str()),
                (afr_supervisor::config::ENV_RUNNER_TOKEN, run.token.as_str()),
                (afr_supervisor::config::ENV_STREAM_URL, origin.as_str()),
                (afr_supervisor::config::ENV_STREAM_CA, root.as_str()),
            ]);
            let config = afr_supervisor::config::Config::from_env(&env)
                .expect("the native fixture configuration is valid");
            let session = afr_supervisor::test_util::Session::connect(&config)
                .await
                .expect("the native fixture connects");
            session.capacity(1);
            session
        })
        .await
}
