//! The webhook addresses Save hands back are ones the router serves.
#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use afd_core::id::Uuid7;
use afd_fleet_runtime::FleetConfig;
use serde_json::json;

use super::webhook_urls;

/// The deployment every address is rendered against.
const DEPLOYMENT: &str = "https://api.example.test";
/// The fleet every address names.
const FLEET: &str = "01920000-0000-7000-8000-000000000001";

/// A config whose only triggers are `triggers`.
fn config(triggers: &serde_json::Value) -> FleetConfig {
    FleetConfig::stored(
        &json!({
            "name": "webhook-render",
            "x-agentsfleet": {
                "triggers": triggers,
                "tools": [],
                "credentials": [],
                "network": {"allow": []},
                // pin test: literal is the contract
                "budget": {"daily_nanos": "1000000000"}
            }
        })
        .to_string(),
    )
    .expect("a valid config")
}

/// `(source, url)` for every address `triggers` earns.
fn rendered(triggers: &serde_json::Value) -> Vec<(String, String)> {
    let fleet = Uuid7::parse(FLEET).expect("a UUIDv7");
    webhook_urls(DEPLOYMENT, &fleet, &config(triggers))
        .into_iter()
        .map(|address| (address.source.into_owned(), address.url.into_owned()))
        .collect()
}

/// GitHub keeps its own route, every other shipped scheme gets the per-source
/// route, one address per trigger, and a Svix-signed source gets the Svix
/// route, the only one that can verify it.
#[test]
fn each_webhook_source_gets_the_address_its_route_serves() {
    let addresses = rendered(&json!([
        {"type": "webhook", "source": "github"},
        {"type": "webhook", "source": "linear"},
        {"type": "webhook", "source": "slack"},
        {"type": "webhook", "source": "stripe", "signature": {"secret_ref": "stripe_svix"}},
        {"type": "cron", "schedule": "0 9 * * *"}
    ]));
    assert_eq!(
        addresses,
        [
            ("github", format!("{DEPLOYMENT}/v1/webhooks/{FLEET}/github")),
            ("linear", format!("{DEPLOYMENT}/v1/webhooks/{FLEET}/linear")),
            ("slack", format!("{DEPLOYMENT}/v1/webhooks/{FLEET}/slack")),
            ("stripe", format!("{DEPLOYMENT}/v1/webhooks/svix/{FLEET}")),
        ]
        .map(|(source, url)| (source.to_owned(), url))
    );
}

/// A fleet woken only by a schedule is handed no webhook address at all.
#[test]
fn a_fleet_with_no_webhook_trigger_gets_no_address() {
    assert_eq!(
        rendered(&json!([{"type": "cron", "schedule": "0 9 * * *"}])),
        Vec::<(String, String)>::new()
    );
}
