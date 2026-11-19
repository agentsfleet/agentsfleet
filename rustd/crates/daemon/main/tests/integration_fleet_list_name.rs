//! M252 §3 — the workspace's fleet list finds one fleet by its exact saved
//! name, inside the workspace asked about.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use agentsfleetd::supervisor::Supervisor;
use serde_json::Value;

use crate::e2e::unique_ids;
use crate::e2e_seed::{FLEET_CONFIG_JSON, Seeded, scenario_tag, seed_fleet_as};
use crate::integration_tool_trace::{tenant_get, tenant_reader};

/// The fleet ids a list page names; the tenant list calls the field `id`.
fn ids(page: &Value) -> Vec<String> {
    page["items"]
        .as_array()
        .expect("a page carries items")
        .iter()
        .map(|item| {
            item["id"]
                .as_str()
                .expect("an item names its fleet")
                .to_owned()
        })
        .collect()
}

/// Dimension 3.1. `?name=agentbob` finds workspace A's `agentbob` alone; a
/// prefix finds nothing; no name lists both of A's.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleet_list_finds_one_fleet_by_exact_name() {
    let mut supervisor = Supervisor::new();
    let (run, token) = tenant_reader(&mut supervisor).await;
    let now = af_core::clock::now();
    let mut seeded = Vec::new();
    for name in ["agentbob", "agentbob-2"] {
        let (fleet, _workspace, _tenant) = unique_ids();
        let tag = scenario_tag(&fleet);
        let seed = Seeded {
            name,
            status: "active",
            tag: &tag,
        };
        seed_fleet_as(
            &run.booted,
            &fleet,
            &run.workspace,
            &run.tenant,
            FLEET_CONFIG_JSON,
            now,
            seed,
        )
        .await;
        seeded.push(fleet);
    }
    let (other, other_workspace, other_tenant) = unique_ids();
    let tag = scenario_tag(&other);
    let seed = Seeded {
        name: "agentbob",
        status: "active",
        tag: &tag,
    };
    seed_fleet_as(
        &run.booted,
        &other,
        &other_workspace,
        &other_tenant,
        FLEET_CONFIG_JSON,
        now,
        seed,
    )
    .await;
    #[expect(
        clippy::disallowed_methods,
        reason = "a plain client calls the daemon this test booted; a proxy rule and a deadline are not what the test proves"
    )]
    let http = reqwest::Client::new();
    let list = format!("/v1/workspaces/{}/fleets", run.workspace);

    let named = tenant_get(&http, &run, &token, &format!("{list}?name=agentbob")).await;
    let agentbob = seeded.first().expect("agentbob was seeded");
    assert_eq!(ids(&named), vec![agentbob.to_string()], "{named}");
    let prefix = tenant_get(&http, &run, &token, &format!("{list}?name=agent")).await;
    assert!(ids(&prefix).is_empty(), "{prefix}");
    let all = ids(&tenant_get(&http, &run, &token, &list).await);
    for fleet in &seeded {
        assert!(all.contains(&fleet.to_string()), "{fleet} missing: {all:?}");
    }
    assert!(!all.contains(&other.to_string()));

    for fleet in seeded.iter().chain([&other]) {
        crate::e2e_retire::retire_fleet(&run.booted, fleet).await;
    }
    supervisor.shutdown().await;
    run.cleanup().await;
}
