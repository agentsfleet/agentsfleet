//! Which hosts `network.allow` admits: each entry read as the sandbox's kernel
//! set reads it, so a request reaches exactly the hosts the sandbox does.

use super::Admission;
use super::tests::draft;
use crate::error::raise;
use crate::fixture::{Shown, policy, shown};

/// A request to a public host no fixture names.
const BILLING: &str = "https://billing.example.net/v1/customers";
/// A public address, spelled as a URL brackets it.
const PUBLIC_V6: &str = "[2606:4700:4700::1111]";

/// The URL `url` is admitted to when `network.allow` lists `allow` alone, or
/// why not.
fn admit(allow: &[&str], url: &str) -> Result<String, Shown> {
    let mut policy = policy(false);
    policy.network_policy.allow = allow.iter().map(|entry| (*entry).into()).collect();
    Admission::new(&policy)
        .admit(draft("GET", url, &[], None))
        .map(|admitted| admitted.url.to_string())
        .map_err(|error| shown(&error))
}

/// Each `(url, host)` is refused as a host the allowlist does not name.
fn assert_unlisted(allow: &[&str], refused: &[(&str, &str)]) {
    for (url, host) in refused {
        assert_eq!(
            admit(allow, url),
            Err(shown(&raise::host_not_allowed(host))),
            "{url}"
        );
    }
}

#[test]
fn should_admit_the_host_an_entry_names_beside_its_scheme_port_and_path() {
    for entry in [
        "https://billing.example.net:443/v1/charges",
        "billing.example.net:443",
        "user@billing.example.net/v1",
    ] {
        assert_eq!(admit(&[entry], BILLING), Ok(BILLING.to_owned()), "{entry}");
    }
}

#[test]
fn should_admit_the_host_a_mixed_case_entry_names() {
    for entry in ["Billing.Example.NET", "https://Billing.Example.NET:443"] {
        assert_eq!(admit(&[entry], BILLING), Ok(BILLING.to_owned()), "{entry}");
    }
}

#[test]
fn should_admit_the_address_a_bracketed_v6_entry_names() {
    let url = format!("https://{PUBLIC_V6}/dns-query");

    for entry in [
        PUBLIC_V6,
        "[2606:4700:4700::1111]:443",
        "https://[2606:4700:4700:0:0:0:0:1111]/",
    ] {
        assert_eq!(admit(&[entry], &url), Ok(url.clone()), "{entry}");
    }
}

#[test]
fn should_admit_nothing_by_an_entry_no_host_can_be_read_from() {
    let allow = [
        "https://",
        "https://example.com:port",
        "https://[2606:4700:4700::1001",
        "billing.example.net",
    ];

    assert_eq!(admit(&allow, BILLING), Ok(BILLING.to_owned()));
    assert_unlisted(
        &allow,
        &[
            ("https://example.com/", "example.com"),
            ("https://[2606:4700:4700::1001]/", "[2606:4700:4700::1001]"),
        ],
    );
}

#[test]
fn should_refuse_a_host_no_entry_names() {
    assert_unlisted(
        &["https://billing.example.net:443/v1", PUBLIC_V6],
        &[
            ("https://evil.example/", "evil.example"),
            ("https://example.net/", "example.net"),
            (
                "https://files.billing.example.net/",
                "files.billing.example.net",
            ),
            ("https://[2606:4700:4700::1001]/", "[2606:4700:4700::1001]"),
        ],
    );
}
