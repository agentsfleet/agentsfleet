//! Optional boot settings and all-or-none configuration groups.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::env::MapEnv;
use agentsfleetd::preflight::{
    API_URL_KNOB, APP_URL_KNOB, ENCRYPTION_MASTER_KEY_KNOB, Fault, PLATFORM_ADMIN_WORKSPACE_KNOB,
    R2_ACCESS_KEY_ID_KNOB, R2_ACCOUNT_ID_KNOB, R2_BUCKET_KNOB, R2_SECRET_ACCESS_KEY_KNOB,
    preflight,
};

const DATABASE_KNOB: &str = "DATABASE_URL_API";
const REDIS_KNOB: &str = "REDIS_URL_API";
const GOOD_DATABASE: &str = "postgres://afd:afd@127.0.0.1:5432/agentsfleet";
const GOOD_REDIS: &str = "redis://127.0.0.1:6379";
const GOOD_KEK: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const PLATFORM_WORKSPACE: &str = "019329c5-0000-7000-8000-0000000000a1";

fn with_optional<'a>(optional: impl IntoIterator<Item = (&'a str, &'a str)>) -> MapEnv {
    MapEnv::from_pairs(
        [
            (DATABASE_KNOB, GOOD_DATABASE),
            (REDIS_KNOB, GOOD_REDIS),
            (ENCRYPTION_MASTER_KEY_KNOB, GOOD_KEK),
        ]
        .into_iter()
        .chain(crate::support::SESSION_PEPPER)
        .chain(crate::support::IDENTITY)
        .chain(optional),
    )
}

#[test]
fn unset_optional_settings_resolve_to_documented_defaults() {
    let config = preflight(&with_optional([])).expect("the required environment boots");

    assert_eq!(config.app_url(), "https://app.agentsfleet.net");
    assert_eq!(config.api_url(), "https://api.agentsfleet.net");
    assert_eq!(config.sse_max_streams(), 64);
    assert!(config.posthog().is_none());
    assert!(config.bundles().is_none());
    assert!(config.platform_admin_workspace().is_none());
    assert!(config.identity().jwks_url.is_none());
}

#[test]
fn complete_optional_settings_survive_preflight() {
    let config = preflight(&with_optional([
        (APP_URL_KNOB, "https://dashboard.example.test"),
        (API_URL_KNOB, "https://api.example.test"),
        ("SSE_MAX_STREAMS", "7"),
        ("POSTHOG_API_KEY", "ph_fixture"),
        ("POSTHOG_HOST", "https://events.example.test"),
        (PLATFORM_ADMIN_WORKSPACE_KNOB, PLATFORM_WORKSPACE),
        (R2_ACCOUNT_ID_KNOB, "account"),
        (R2_ACCESS_KEY_ID_KNOB, "access"),
        (R2_SECRET_ACCESS_KEY_KNOB, "secret"),
        (R2_BUCKET_KNOB, "snapshots"),
        ("OIDC_JWKS_URL", "https://identity.example.test/jwks"),
    ]))
    .expect("complete optional groups are accepted");

    assert_eq!(config.app_url(), "https://dashboard.example.test");
    assert_eq!(config.api_url(), "https://api.example.test");
    assert_eq!(config.sse_max_streams(), 7);
    let analytics = config.posthog().expect("the analytics key enables output");
    assert_eq!(analytics.project_key.as_ref(), "ph_fixture");
    assert_eq!(
        analytics.host.as_deref(),
        Some("https://events.example.test")
    );
    assert_eq!(
        config.platform_admin_workspace().map(ToString::to_string),
        Some(PLATFORM_WORKSPACE.to_owned())
    );
    let bundles = config
        .bundles()
        .expect("all four R2 knobs build one config");
    assert_eq!(
        bundles.endpoint(),
        "https://account.r2.cloudflarestorage.com"
    );
    assert_eq!(bundles.bucket.as_ref(), "snapshots");
    assert_eq!(agentsfleetd::preflight::BundleStoreConfig::region(), "auto");
    assert_eq!(
        config.identity().jwks_url.as_deref(),
        Some("https://identity.example.test/jwks")
    );
}

#[test]
fn invalid_optionals_are_reported_together() {
    let refusal = preflight(&with_optional([
        ("SSE_MAX_STREAMS", "0"),
        (PLATFORM_ADMIN_WORKSPACE_KNOB, "not-a-workspace"),
        (R2_ACCOUNT_ID_KNOB, "account"),
    ]))
    .expect_err("invalid optional settings still refuse boot");

    assert!(refusal.knobs().contains(&"SSE_MAX_STREAMS"));
    assert!(refusal.knobs().contains(&PLATFORM_ADMIN_WORKSPACE_KNOB));
    for missing in [
        R2_ACCESS_KEY_ID_KNOB,
        R2_SECRET_ACCESS_KEY_KNOB,
        R2_BUCKET_KNOB,
    ] {
        assert!(refusal.knobs().contains(&missing));
    }
    assert!(refusal.faults().iter().any(|fault| matches!(
        fault,
        Fault::Invalid {
            knob: "SSE_MAX_STREAMS",
            ..
        }
    )));
}
