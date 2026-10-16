//! Optional boot settings and all-or-none configuration groups.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::env::MapEnv;
use afd_core::error_code;
use agentsfleetd::BootFailure;
use agentsfleetd::preflight::{
    API_URL_KNOB, APP_URL_KNOB, ENCRYPTION_MASTER_KEY_KNOB, Fault, PLATFORM_ADMIN_WORKSPACE_KNOB,
    PROVIDER_API_BASE_KNOB, SLACK_API_URL_KNOB, preflight,
};

const DATABASE_KNOB: &str = "DATABASE_URL_API";
const DRAGONFLY_KNOB: &str = "DRAGONFLY_URL";
const GOOD_DATABASE: &str = "postgres://afd:afd@127.0.0.1:5432/agentsfleet";
const GOOD_DRAGONFLY_URL: &str = "redis://127.0.0.1:6379";
const GOOD_KEK: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const PLATFORM_WORKSPACE: &str = "019329c5-0000-7000-8000-0000000000a1";

fn with_optional<'a>(optional: impl IntoIterator<Item = (&'a str, &'a str)>) -> MapEnv {
    MapEnv::from_pairs(
        [
            (DATABASE_KNOB, GOOD_DATABASE),
            (DRAGONFLY_KNOB, GOOD_DRAGONFLY_URL),
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

    // The parsed form, which carries the root path's slash.
    assert_eq!(config.app_url().as_str(), "https://app.agentsfleet.net/");
    assert_eq!(config.api_url(), "https://api.agentsfleet.net");
    assert_eq!(config.sse_max_streams(), 256);
    assert!(config.posthog().is_none());
    assert!(config.platform_admin_workspace().is_none());
    assert!(config.identity().jwks_url.is_none());
    assert_eq!(
        config.slack_api_base(),
        afd_connector::slack::SLACK_API_BASE,
        "every answer and interim line goes to slack.com unless told otherwise"
    );
}

/// A Slack base carries every workspace's bot token, so it is https, or http
/// only on a loopback host; anything else refuses boot rather than sending a
/// token in the clear or to a host nobody meant. A query or a fragment refuses
/// too, since the method path appended after it would not name the method.
#[test]
fn a_slack_base_follows_its_knob_only_where_a_token_is_safe() {
    for safe in [
        "https://slack.example.test/api",
        "http://127.0.0.1:9/api",
        "http://localhost:9/api",
        "http://[::1]:9/api",
    ] {
        let config = preflight(&with_optional([(SLACK_API_URL_KNOB, safe)]))
            .unwrap_or_else(|refused| panic!("{safe} boots: {refused:?}"));
        assert_eq!(config.slack_api_base(), safe);
    }
    for unsafe_base in [
        "http://slack.example.test/api",
        "http://10.0.0.7/api",
        "ftp://127.0.0.1/api",
        "not a url",
        "https://slack.example.test/api?team=1",
        "https://slack.example.test/api#methods",
    ] {
        let refusal = preflight(&with_optional([(SLACK_API_URL_KNOB, unsafe_base)]))
            .expect_err("an unsafe base refuses boot");
        assert!(
            refusal.faults().iter().any(|fault| matches!(
                fault,
                Fault::Invalid {
                    knob: SLACK_API_URL_KNOB,
                    ..
                }
            )),
            "{unsafe_base}: {refusal:?}"
        );
        assert!(
            !format!("{refusal:?}").contains(unsafe_base),
            "the refusal never echoes the value"
        );
    }
}

/// Every capability read sends the provider secret key to `CLERK_API_BASE`, so
/// it is https, or http only on a loopback host; anything else refuses boot
/// rather than sending the secret in the clear or to a host nobody meant.
#[test]
fn a_provider_base_boots_only_where_its_secret_is_safe() {
    for safe in [
        "https://api.clerk.example.test/v1",
        "http://127.0.0.1:9/v1",
        "http://localhost:9/v1",
    ] {
        preflight(&with_optional([(PROVIDER_API_BASE_KNOB, safe)]))
            .unwrap_or_else(|refused| panic!("{safe} boots: {refused:?}"));
    }
    for unsafe_base in [
        "http://api.clerk.example.test/v1",
        "http://10.0.0.7/v1",
        "ftp://127.0.0.1/v1",
        "not a url",
    ] {
        let refusal = preflight(&with_optional([(PROVIDER_API_BASE_KNOB, unsafe_base)]))
            .expect_err("an unsafe base refuses boot");
        assert!(
            refusal.faults().iter().any(|fault| matches!(
                fault,
                Fault::Invalid {
                    knob: PROVIDER_API_BASE_KNOB,
                    ..
                }
            )),
            "{unsafe_base}: {refusal:?}"
        );
        assert!(
            !format!("{refusal:?}").contains(unsafe_base),
            "the refusal never echoes the value"
        );
    }
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
        ("OIDC_JWKS_URL", "https://identity.example.test/jwks"),
    ]))
    .expect("complete optional groups are accepted");

    assert_eq!(config.app_url().as_str(), "https://dashboard.example.test/");
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
    ]))
    .expect_err("invalid optional settings still refuse boot");

    assert!(refusal.knobs().contains(&"SSE_MAX_STREAMS"));
    assert!(refusal.knobs().contains(&PLATFORM_ADMIN_WORKSPACE_KNOB));
    assert!(refusal.faults().iter().any(|fault| matches!(
        fault,
        Fault::Invalid {
            knob: "SSE_MAX_STREAMS",
            ..
        }
    )));
}

/// A dashboard base no page can hang off refuses boot, naming the knob, rather
/// than booting a daemon whose every connect and invite link would fail. That
/// includes a base that is not a bare http(s) URL, which is what the refusal
/// promises.
///
/// The successor of `afd_api`'s `a_dashboard_base_that_is_not_a_url_is_refused_
/// rather_than_relayed_to`: the base used to be parsed per connect and refused
/// there as `UZ-CONN-001`. It is parsed once, here, so the guarantee lives
/// here, as the boot refusal's own code.
#[test]
fn a_dashboard_base_that_is_not_a_url_refuses_boot() {
    for base in [
        "not a url at all",
        "/relative",
        "mailto:ops@example.test",
        "ftp://dashboard.example.test",
        "file:///srv/dashboard",
        "https://u:p@dashboard.example.test",
        "https://dashboard.example.test/?q=1",
        "https://dashboard.example.test/#f",
    ] {
        let refusal = preflight(&with_optional([(APP_URL_KNOB, base)]))
            .expect_err("an unusable dashboard base refuses boot");
        assert_eq!(
            refusal.knobs(),
            [APP_URL_KNOB],
            "{base}: only the dashboard"
        );
        assert!(
            matches!(
                refusal.faults(),
                [Fault::Invalid { why, .. }] if why.contains("http(s) URL")
            ),
            "{base}: {:?}",
            refusal.faults()
        );
        assert_eq!(
            BootFailure::from(refusal).code(),
            error_code::STARTUP_ENV_CHECK,
            "{base}: refused as the environment, before anything opens"
        );
    }
}

/// A blank or padded optional knob reads as the trimmed value, or as unset:
/// a blank analytics key is analytics off, never a fault, and a padded key
/// (a heredoc's trailing newline) is still that key.
#[test]
fn blank_optionals_read_as_unset_and_padded_ones_as_their_value() {
    let blank = preflight(&with_optional([
        ("POSTHOG_API_KEY", "   "),
        ("POSTHOG_HOST", "https://events.example.test"),
        ("OIDC_JWKS_URL", " "),
        ("SSE_MAX_STREAMS", ""),
    ]))
    .expect("blank optionals are unset, not faults");
    assert!(blank.posthog().is_none());
    assert!(blank.identity().jwks_url.is_none());
    assert_eq!(blank.sse_max_streams(), 256);

    let padded = preflight(&with_optional([
        ("POSTHOG_API_KEY", " ph_fixture\n"),
        ("POSTHOG_HOST", "  "),
        (APP_URL_KNOB, " https://dashboard.example.test "),
    ]))
    .expect("padded optionals are their trimmed values");
    let analytics = padded.posthog().expect("a padded key is still a key");
    assert_eq!(analytics.project_key.as_ref(), "ph_fixture");
    assert_eq!(
        analytics.host, None,
        "a blank host keeps the client default"
    );
    assert_eq!(padded.app_url().as_str(), "https://dashboard.example.test/");
}
