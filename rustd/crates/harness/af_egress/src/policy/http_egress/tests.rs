//! Broader host scope never grants credentials, private addresses or writes.

use af_wire::policy::{HttpEgress, PACKAGE_MANAGER_DOMAINS};

use super::EgressPolicy;
use super::tests::draft;
use crate::fixture::{policy, shown};
use crate::refusal::Refusal;

const UNLISTED: &str = "https://downloads.example.net/log.txt";

#[test]
fn should_allow_public_hosts_only_when_selected() -> crate::Result<()> {
    let mut policy = policy();
    policy.network_policy.allow.clear();
    let refused = EgressPolicy::new(&policy)
        .allow(draft("GET", UNLISTED, &[], None))
        .err();
    assert_eq!(
        refused.as_ref().and_then(crate::Error::refusal),
        Some(Refusal::HostNotAllowed)
    );
    assert_eq!(
        refused.as_ref().map(crate::Error::code),
        Some(af_core::error_code::INVALID_REQUEST)
    );
    assert!(matches!(
        refused.as_ref().map(crate::Error::kind),
        Some(crate::error::ErrorKind::HostNotAllowed { host }) if host == "downloads.example.net"
    ));
    policy.network_policy.http_egress = HttpEgress::Public;
    EgressPolicy::new(&policy).allow(draft("GET", UNLISTED, &[], None))?;
    Ok(())
}

#[test]
fn should_allow_only_exact_preset_and_additional_hosts_in_restricted_mode() -> crate::Result<()> {
    let mut policy = policy();
    policy.network_policy.allow = vec!["extra.example.net".into()];
    policy.network_policy.package_manager_domains = true;
    let egress_policy = EgressPolicy::new(&policy);
    for host in PACKAGE_MANAGER_DOMAINS
        .into_iter()
        .chain(["extra.example.net"])
    {
        egress_policy.allow(draft("GET", &format!("https://{host}/index"), &[], None))?;
    }
    for host in [
        "other.example.net",
        "sub.registry.npmjs.org",
        "registry.npmjs.org.evil.example",
    ] {
        let refusal = egress_policy
            .allow(draft("GET", &format!("https://{host}/"), &[], None))
            .err();
        assert_eq!(
            refusal.as_ref().and_then(crate::Error::refusal),
            Some(Refusal::HostNotAllowed)
        );
    }
    Ok(())
}

#[test]
fn should_preserve_boundaries_with_public_http_egress() {
    let mut policy = policy();
    policy.network_policy.http_egress = HttpEgress::Public;
    let egress_policy = EgressPolicy::new(&policy);
    let cases = [
        (
            draft("GET", "http://downloads.example.net/", &[], None),
            Refusal::HttpsRequired,
        ),
        (
            draft("GET", "https://user:pass@downloads.example.net/", &[], None),
            Refusal::PlacementNotAllowed,
        ),
        (
            draft("GET", "https://127.0.0.1/", &[], None),
            Refusal::EgressDenied,
        ),
        (
            draft("GET", "https://169.254.169.254/", &[], None),
            Refusal::EgressDenied,
        ),
        (
            draft("GET", "https://[::1]/", &[], None),
            Refusal::EgressDenied,
        ),
    ];
    for (draft, expected) in cases {
        let refused = egress_policy.allow(draft).err();
        assert_eq!(
            refused.as_ref().and_then(crate::Error::refusal),
            Some(expected)
        );
    }
}

#[test]
fn should_restrict_credentials_and_repository_with_public_http_egress() {
    let mut policy = policy();
    policy.network_policy.http_egress = HttpEgress::Public;
    let egress_policy = EgressPolicy::new(&policy);
    let cases = [
        (
            draft(
                "GET",
                UNLISTED,
                &[("Authorization", "Bearer ${secrets.github.token}")],
                None,
            ),
            Refusal::CredentialHostNotAllowed,
        ),
        (
            draft(
                "GET",
                UNLISTED,
                &[("Authorization", "Bearer ${secrets.grafana.token}")],
                None,
            ),
            Refusal::CredentialHostNotAllowed,
        ),
        (
            draft(
                "GET",
                "https://api.github.com/repos/acme/other/pulls",
                &[],
                None,
            ),
            Refusal::RequestPolicyNotAllowed,
        ),
    ];
    for (draft, expected) in cases {
        let refused = egress_policy.allow(draft).err();
        assert_eq!(
            refused.as_ref().and_then(crate::Error::refusal),
            Some(expected),
            "{:?}",
            refused.as_ref().map(shown)
        );
    }
}

/// A public fleet may write to any public host: no method is held back for a
/// host it names no credential for.
#[test]
fn should_allow_a_credential_free_post_to_an_unlisted_host_with_public_http_egress() {
    let mut policy = policy();
    policy.network_policy.http_egress = HttpEgress::Public;

    let sent = EgressPolicy::new(&policy).allow(draft("POST", UNLISTED, &[], Some("{}")));
    assert!(sent.is_ok(), "{:?}", sent.err().as_ref().map(shown));

    policy.network_policy.http_egress = HttpEgress::Restricted;
    let refused = EgressPolicy::new(&policy)
        .allow(draft("POST", UNLISTED, &[], Some("{}")))
        .err();
    assert_eq!(
        refused.as_ref().and_then(crate::Error::refusal),
        Some(Refusal::HostNotAllowed)
    );
}

#[test]
fn the_dns_root_label_preserves_host_approval_credentials_and_origin_rules() -> crate::Result<()> {
    let mut policy = policy();
    for origin in &mut policy.http_origin_policies {
        origin.host = format!("{}.", origin.host).into();
    }
    let guard = EgressPolicy::new(&policy);
    let github = [("Authorization", "Bearer ${secrets.github.token}")];
    let grafana = [("Authorization", "Bearer ${secrets.grafana.token}")];
    for host in ["api.github.com", "API.GITHUB.COM."] {
        let url = format!("https://{host}/repos/acme/widgets/pulls");
        let allowed = guard.allow(draft("GET", &url, &github, None))?;
        assert_eq!(allowed.url.host_str(), Some("api.github.com"));
        let denied = guard.allow(draft("GET", &url, &grafana, None)).err();
        assert_eq!(
            denied.as_ref().and_then(crate::Error::refusal),
            Some(Refusal::CredentialHostNotAllowed)
        );
        let outside = format!("https://{host}/repos/acme/other/pulls");
        let denied = guard.allow(draft("GET", &outside, &[], None)).err();
        assert_eq!(
            denied.as_ref().and_then(crate::Error::refusal),
            Some(Refusal::RequestPolicyNotAllowed)
        );
    }
    for host in ["demo-grafana.internal", "demo-grafana.internal."] {
        let allowed = guard.allow(draft(
            "GET",
            &format!("https://{host}/dashboard"),
            &grafana,
            None,
        ))?;
        assert_eq!(allowed.url.host_str(), Some("demo-grafana.internal"));
    }
    Ok(())
}
