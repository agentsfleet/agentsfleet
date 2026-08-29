use std::time::Duration;

use serde_json::json;

use super::{Answer, Counting, IN_AN_HOUR_MS, INTEGRATION, NOW_MS, ask, broker, stored, workspace};
use crate::credential::broker::{Ask, Exchanger as _, Vendors};
use crate::credential::outcome::{Outcome, Retry};
use crate::credential::platform::{GithubApp, OauthApp, Platform};
use crate::secrets::connector::{Connectors as _, Registry};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_handle_naming_nothing_this_registry_carries_is_its_own_answer() {
    let vendor = Counting::new(Answer::Token { rotates: false }, Duration::ZERO);
    let broker = broker(&vendor);
    let workspace = workspace();

    for unknown in [
        json!({INTEGRATION: "githubb"}),
        json!({"api_token": "no integration field"}),
        json!(["not an object"]),
    ] {
        let outcome = broker.mint(ask(&workspace, &unknown, None)).await;
        assert!(
            matches!(outcome, Outcome::UnknownIntegration),
            "{unknown}: {outcome:?}"
        );
    }
    assert_eq!(vendor.calls(), 0, "an unresolvable handle reached a vendor");
}

#[test]
fn a_stored_handle_hands_back_what_it_holds_and_refuses_when_it_holds_nothing() {
    let held = stored(&json!({INTEGRATION: "static", "token": "ghp_stored"}));
    let minted = held.minted().expect("a static handle carries its token");
    assert_eq!(minted.token.as_str(), "ghp_stored");
    assert_eq!(minted.expires_at_ms, i64::MAX);
    assert!(minted.rotated_refresh_token.is_none());

    assert!(matches!(
        stored(&json!({INTEGRATION: "static"})),
        Outcome::ReconnectRequired
    ));
}

#[tokio::test]
async fn vendor_dispatch_degrades_only_the_connector_it_cannot_serve() {
    let registry = Registry;
    let client = reqwest::Client::new();
    let empty = Vendors::new(Platform::empty(), client.clone());
    let workspace = workspace();

    for name in ["github", "zoho"] {
        let connector = registry.resolve(name).expect("shipped connector resolves");
        let handle = json!({INTEGRATION: name});
        assert!(matches!(
            empty
                .exchange(connector, ask(&workspace, &handle, None))
                .await,
            Outcome::Unconfigured
        ));
    }

    let configured = Platform::empty()
        .with_github(GithubApp {
            app_id: 7,
            private_key_pem: "not a key".to_owned().into(),
        })
        .with_oauth(
            "zoho",
            OauthApp {
                client_id: "client".to_owned(),
                client_secret: "secret".to_owned().into(),
            },
        );
    let vendors = Vendors::new(configured, client);
    let github = registry.resolve("github").expect("GitHub resolves");
    let github_handle = json!({INTEGRATION: "github", "installation_id": 7});
    let binding = super::binding(afd_fleet_runtime::config::Access::Read);
    assert!(matches!(
        vendors
            .exchange(github, ask(&workspace, &github_handle, Some(&binding)))
            .await,
        Outcome::MintFailed(Retry::Permanent)
    ));

    let zoho = registry.resolve("zoho").expect("Zoho resolves");
    let zoho_handle = json!({INTEGRATION: "zoho"});
    assert!(matches!(
        vendors
            .exchange(zoho, ask(&workspace, &zoho_handle, None))
            .await,
        Outcome::ReconnectRequired
    ));

    let static_connector = registry.resolve("static").expect("static resolves");
    let static_handle = json!({INTEGRATION: "static", "token": "held"});
    assert!(
        empty
            .exchange(
                static_connector,
                Ask {
                    workspace_id: &workspace,
                    handle: &static_handle,
                    binding: None,
                    now_ms: NOW_MS,
                },
            )
            .await
            .minted()
            .is_some()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cached_token_inside_the_expiry_skew_is_reminted() {
    let vendor = Counting::new(Answer::Token { rotates: false }, Duration::ZERO);
    let broker = broker(&vendor);
    let workspace = workspace();
    let handle = super::zoho_handle();

    broker.mint(ask(&workspace, &handle, None)).await;
    let mut expired = ask(&workspace, &handle, None);
    expired.now_ms = IN_AN_HOUR_MS;
    broker.mint(expired).await;

    assert_eq!(vendor.calls(), 2, "the stale cache entry was served");
}
