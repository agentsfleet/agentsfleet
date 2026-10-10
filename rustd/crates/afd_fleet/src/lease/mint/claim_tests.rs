//! What a stored handle claims, against the shipped connector set.
//!
//! The failure each case exists for is a handle the workspace wrote that the
//! broker would mint from without the mint having checked it: an on-demand
//! connector filed under a name the grant gate never checked, a GitHub
//! installation nobody proved this workspace owns, or a refresh that would post
//! the platform's client secret somewhere the connect flow never points it.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::collections::BTreeSet;

use afd_connector::Provider;
use afd_core::error_code::{self, ErrorCode};
use afd_credential::credential::oauth;
use afd_credential::secrets::connector::{Connectors as _, Exchange, Registry};
use serde_json::{Value, json};

use super::{Claim, claim};

/// The connector set the daemon ships.
const SHIPPED: Registry = Registry::SHIPPED;

/// The code `handle`, opened under `requested`, is refused with.
fn refused(requested: &str, handle: &Value) -> ErrorCode {
    claim(SHIPPED, requested, handle)
        .expect_err("the handle must be refused")
        .code()
}

#[test]
fn every_on_demand_connector_is_a_provider_the_routing_table_knows() {
    // The routing row's `provider` column is written from `Provider::id` by the
    // connect flow and read here from the registry's name. Two spellings of one
    // vocabulary, held together by this test rather than by luck: a drift
    // would refuse every legitimate GitHub mint with nothing saying why.
    for provider in Provider::ALL {
        let Some(connector) = SHIPPED.resolve(provider.id()) else {
            // Slack answers mentions and mints nothing: it has no connector.
            assert_eq!(*provider, Provider::Slack);
            continue;
        };
        assert_eq!(connector.name(), provider.id());
        assert!(
            !matches!(connector.exchange(), Exchange::Stored),
            "{provider}"
        );
    }
}

#[test]
fn every_server_a_zoho_callback_writes_is_one_a_refresh_will_post_to() {
    // `afd_connector` maps a callback's `location` to the server it writes; the
    // exchange holds its post to `ZOHO_ACCOUNTS_SERVERS`. A centre added to one
    // and not the other would leave that region's tenants unable to mint, so
    // the two lists are held to each other here, where both crates are seen.
    let locations = [
        None,
        Some("eu"),
        Some("in"),
        Some("au"),
        Some("cn"),
        Some("jp"),
        Some("ca"),
    ];
    let written: BTreeSet<&str> = locations
        .into_iter()
        .map(afd_connector::zoho::accounts_base)
        .collect();
    let posted: BTreeSet<&str> = oauth::ZOHO_ACCOUNTS_SERVERS.into_iter().collect();
    assert_eq!(written, posted);
}

#[test]
fn a_github_handle_under_its_own_name_claims_its_installation() {
    // Every spelling the exchange parses to 42 claims 42: the check and the
    // mint read one number, so a leading zero or a sign cannot make them
    // disagree about which installation is spent.
    for spelled in [json!("42"), json!(42), json!("042"), json!("+42")] {
        let handle = json!({"integration": "github", "installation_id": spelled});

        assert_eq!(
            claim(SHIPPED, "github", &handle).expect("a connected handle reads"),
            Claim::Installation {
                provider: Provider::GitHub,
                id: 42
            },
            "{handle}"
        );
    }
}

#[test]
fn an_on_demand_handle_under_another_name_is_not_a_connection() {
    // The grant gate ran against the name asked for. A GitHub or Zoho handle
    // filed under some other name would otherwise mint as a connector whose
    // grant nobody checked.
    for (requested, handle) in [
        (
            "mything",
            json!({"integration": "github", "installation_id": "42"}),
        ),
        (
            "github",
            json!({"integration": "zoho", "refresh_token": "rt"}),
        ),
        (
            "zoho",
            json!({"integration": "jira", "refresh_token": "rt"}),
        ),
    ] {
        assert_eq!(
            refused(requested, &handle),
            error_code::CRED_INTEGRATION_NOT_CONNECTED,
            "{requested}: {handle}"
        );
    }
}

#[test]
fn a_github_handle_naming_no_readable_installation_asks_for_a_reconnect() {
    // The same answer the exchange gives it, so moving the read earlier
    // changes no runner-visible code for a handle that was always broken.
    for handle in [
        json!({"integration": "github"}),
        json!({"integration": "github", "installation_id": "not-a-number"}),
        json!({"integration": "github", "installation_id": {"id": 42}}),
    ] {
        assert_eq!(
            refused("github", &handle),
            error_code::GH_RECONNECT_REQUIRED,
            "{handle}"
        );
    }
}

#[test]
fn a_refresh_posts_only_where_the_connect_flow_points_it() {
    // What a Zoho callback writes, and the providers that write no base at all.
    for (requested, handle) in [
        (
            "zoho",
            json!({"integration": "zoho", "refresh_token": "rt", "accounts_base": "https://accounts.zoho.eu"}),
        ),
        (
            "zoho",
            json!({"integration": "zoho", "refresh_token": "rt", "accounts_base": "https://accounts.zohocloud.ca"}),
        ),
        (
            "zoho",
            json!({"integration": "zoho", "refresh_token": "rt"}),
        ),
        // A path is never read, so naming one moves nothing.
        (
            "zoho",
            json!({"integration": "zoho", "refresh_token": "rt", "accounts_base": "https://accounts.zoho.com", "token_path": "/collect"}),
        ),
        (
            "jira",
            json!({"integration": "jira", "refresh_token": "rt", "cloud_id": "c"}),
        ),
        (
            "linear",
            json!({"integration": "linear", "refresh_token": "rt"}),
        ),
    ] {
        assert_eq!(
            claim(SHIPPED, requested, &handle).expect("the connect flow's own shape"),
            Claim::Unowned,
            "{handle}"
        );
    }
}

#[test]
fn a_refresh_steered_anywhere_else_never_reaches_the_exchange() {
    // Each would post this deployment's client secret to a host or path the
    // connect flow never writes: a public host the SSRF guard admits, a Zoho
    // lookalike, and a real Zoho server named by a provider that has none.
    for (requested, handle) in [
        (
            "zoho",
            json!({"integration": "zoho", "refresh_token": "rt", "accounts_base": "https://collector.example"}),
        ),
        (
            "zoho",
            json!({"integration": "zoho", "refresh_token": "rt", "accounts_base": "https://accounts.zoho.com.example"}),
        ),
        (
            "jira",
            json!({"integration": "jira", "refresh_token": "rt", "accounts_base": "https://accounts.zoho.com"}),
        ),
    ] {
        assert_eq!(
            refused(requested, &handle),
            error_code::CONNECTOR_OAUTH_EXCHANGE_FAILED,
            "{handle}"
        );
    }
}

#[test]
fn handles_with_nothing_to_own_pass_through_to_the_broker() {
    // Inline credentials are the workspace's own bytes, and an unknown or
    // absent integration is the broker's `UnknownIntegration` to answer —
    // none has an owner a routing row could disagree with.
    for (requested, handle) in [
        ("pat", json!({"integration": "static", "token": "ghp_own"})),
        (
            "github",
            json!({"integration": "static", "token": "ghp_own"}),
        ),
        ("github", json!({"webhook_secret": "whsec"})),
        ("acme", json!({"integration": "acme"})),
        ("github", json!({"integration": 7})),
        ("github", json!(["github"])),
    ] {
        assert_eq!(
            claim(SHIPPED, requested, &handle).expect("nothing to refuse"),
            Claim::Unowned,
            "{requested}: {handle}"
        );
    }
}
