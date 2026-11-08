//! Identity routing is an explicit deployment choice, never ambient discovery.

#![expect(
    clippy::expect_used,
    reason = "test setup fails loudly on an invalid fixture"
)]

use af_core::env::MapEnv;
use afd_identity::Transport;

use super::{Fault, IDENTITY_HTTP_PROXY_URL_KNOB, read};

#[test]
fn an_unset_or_blank_proxy_keeps_identity_direct_despite_ambient_proxies() {
    for value in [None, Some(""), Some("   ")] {
        let mut pairs = vec![("HTTPS_PROXY", "http://127.0.0.1:9")];
        if let Some(value) = value {
            pairs.push((IDENTITY_HTTP_PROXY_URL_KNOB, value));
        }
        let mut faults = Vec::new();
        let route = read::identity_transport(&MapEnv::from_pairs(pairs), &mut faults);
        assert!(matches!(route, Some(Transport::Direct)));
        assert_eq!(faults, []);
    }
}

#[test]
fn only_an_explicit_http_proxy_selects_the_trusted_route() {
    for url in [" http://proxy:8080 \n", "https://proxy.example.test:8443"] {
        let mut faults = Vec::new();
        let env = MapEnv::from_pairs([(IDENTITY_HTTP_PROXY_URL_KNOB, url)]);
        let route = read::identity_transport(&env, &mut faults);
        assert!(matches!(route, Some(Transport::TrustedProxy(_))));
        assert_eq!(faults, []);
    }
}

#[test]
fn an_invalid_explicit_proxy_refuses_configuration_without_direct_fallback() {
    for raw in [
        "not a URL",
        "socks5://proxy:1080",
        "http://",
        "http://proxy/path",
        "http://proxy/?token=fixture-only",
        "http://proxy/#fragment",
    ] {
        let mut faults = Vec::new();
        let env = MapEnv::from_pairs([(IDENTITY_HTTP_PROXY_URL_KNOB, raw)]);
        assert!(read::identity_transport(&env, &mut faults).is_none());
        assert!(matches!(
            faults.as_slice(),
            [Fault::Invalid {
                knob: IDENTITY_HTTP_PROXY_URL_KNOB,
                ..
            }]
        ));
        assert!(!format!("{faults:?}").contains(raw));
    }
}

#[test]
fn debug_output_never_exposes_proxy_credentials_or_destination() {
    let mut faults = Vec::new();
    let env = MapEnv::from_pairs([(
        IDENTITY_HTTP_PROXY_URL_KNOB,
        "http://proxy-user:fixture-only@private-proxy.example.test:8080",
    )]);
    let route = read::identity_transport(&env, &mut faults);
    assert!(matches!(route, Some(Transport::TrustedProxy(_))));
    assert_eq!(format!("{route:?}"), "Some(TrustedProxy([REDACTED]))");
    assert_eq!(format!("{:?}", Transport::default()), "Direct");
    assert_eq!(faults, []);
    let route = route.expect("configured proxy");
    let deadline = std::time::Duration::from_secs(1);
    let secret = afd_identity::ProviderSecret::new("fixture-only").expect("fixture secret");
    let keys =
        afd_identity::HttpKeySet::new("https://identity.example.test/keys", deadline, &route)
            .expect("a key client");
    let claims = afd_identity::ProviderClaims::new(
        "https://identity.example.test",
        secret.clone(),
        deadline,
        &route,
    )
    .expect("a claim client");
    let metadata = afd_identity::ProviderMetadata::new(
        "https://identity.example.test",
        secret,
        deadline,
        &route,
    )
    .expect("a metadata client");
    for representation in [
        format!("{keys:?}"),
        format!("{claims:?}"),
        format!("{metadata:?}"),
    ] {
        assert!(!representation.contains("private-proxy"));
        assert!(!representation.contains("proxy-user"));
        assert!(!representation.contains("fixture-only"));
    }
}
