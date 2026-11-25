//! A fleet made from a first message runs with the tool catalog and nothing a
//! person has not approved.

use super::access::Live;
use super::*;

use serde_json::Value;

// Value: protects=a bare fleet's first run holds every published tool, no
// secret, no repository binding and a restricted network that allows
// nothing; fails_when=the policy declares a workspace secret unasked, binds a
// repository, or opens the network by default; seam=the gate chain's delivery
// over live Postgres and NATS.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_new_fleets_run_holds_the_catalog_and_no_secret() {
    let fixtures = Fixtures::create().await;
    let live = Live::issue(&fixtures).await;
    let policy = live.payload.get("policy").expect("the run's policy");

    let tools: Vec<&str> = policy
        .get("tools")
        .and_then(Value::as_array)
        .expect("the offered tools")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let catalog = afd_gate::policy::catalog::offered(false);
    assert!(
        catalog.iter().all(|tool| tools.contains(&tool.as_ref())),
        "every published tool is offered: {tools:?}"
    );
    let secrets = policy
        .get("secrets_map")
        .and_then(Value::as_object)
        .map_or(0, serde_json::Map::len);
    assert_eq!(secrets, 0, "no secret is declared");
    assert!(policy.get("repository_binding").is_none_or(Value::is_null));
    assert_eq!(
        policy
            .pointer("/network_policy/http_egress")
            .and_then(Value::as_str),
        Some("restricted")
    );
    assert_eq!(
        policy
            .pointer("/network_policy/allow")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(0),
        "a restricted network with nothing approved allows nothing"
    );
    fixtures.cleanup().await;
}
