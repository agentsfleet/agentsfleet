//! Broader host scope never grants credentials, private addresses or writes.

use afd_wire::policy::{HttpEgress, PACKAGE_MANAGER_DOMAINS};

use super::Admission;
use super::tests::draft;
use crate::fixture::{policy, shown};
use crate::refusal::Refusal;

const UNLISTED: &str = "https://downloads.example.net/log.txt";

#[test]
fn should_admit_public_hosts_only_when_selected() {
    let mut policy = policy(true);
    policy.network_policy.allow.clear();
    assert!(
        Admission::new(&policy)
            .admit(draft("GET", UNLISTED, &[], None))
            .is_err()
    );
    policy.network_policy.http_egress = HttpEgress::Public;
    assert!(
        Admission::new(&policy)
            .admit(draft("GET", UNLISTED, &[], None))
            .is_ok()
    );
}

#[test]
fn should_admit_only_exact_preset_and_additional_hosts_in_restricted_mode() {
    let mut policy = policy(true);
    policy.network_policy.allow = vec!["extra.example.net".into()];
    policy.network_policy.package_manager_domains = true;
    let admission = Admission::new(&policy);
    for host in PACKAGE_MANAGER_DOMAINS
        .into_iter()
        .chain(["extra.example.net"])
    {
        assert!(
            admission
                .admit(draft("GET", &format!("https://{host}/index"), &[], None))
                .is_ok()
        );
    }
    for host in [
        "other.example.net",
        "sub.registry.npmjs.org",
        "registry.npmjs.org.evil.example",
    ] {
        let refusal = admission
            .admit(draft("GET", &format!("https://{host}/"), &[], None))
            .err();
        assert_eq!(
            refusal.as_ref().and_then(crate::Error::refusal),
            Some(Refusal::HostNotAllowed)
        );
    }
}

#[test]
fn should_preserve_boundaries_with_public_http_egress() {
    let mut policy = policy(true);
    policy.network_policy.http_egress = HttpEgress::Public;
    let admission = Admission::new(&policy);
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
            Refusal::AddressNotAllowed,
        ),
        (
            draft("GET", "https://169.254.169.254/", &[], None),
            Refusal::AddressNotAllowed,
        ),
        (
            draft("GET", "https://[::1]/", &[], None),
            Refusal::AddressNotAllowed,
        ),
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
        (
            draft("POST", UNLISTED, &[], Some("{}")),
            Refusal::MethodNotAllowed,
        ),
    ];
    for (draft, expected) in cases {
        let refused = admission.admit(draft).err();
        assert_eq!(
            refused.as_ref().and_then(crate::Error::refusal),
            Some(expected),
            "{:?}",
            refused.as_ref().map(shown)
        );
    }
}
