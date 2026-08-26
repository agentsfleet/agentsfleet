//! The exact admin/operator route and verb inventory served by M179.
#![cfg(feature = "test-util")]

use std::collections::BTreeSet;

use afd_api::route::{AdminRoute, RunnerOpsRoute, TenantRoute, Verb};

type Endpoint = (&'static str, Verb);

const EXPECTED: &[Endpoint] = &[
    ("/v1/admin/fleet-libraries", Verb::Get),
    ("/v1/admin/fleet-libraries", Verb::Post),
    ("/v1/admin/fleet-libraries/{id}", Verb::Patch),
    ("/v1/admin/fleet-libraries/{id}", Verb::Delete),
    ("/v1/admin/platform-keys", Verb::Get),
    ("/v1/admin/platform-keys", Verb::Put),
    ("/v1/admin/platform-keys/{provider}", Verb::Delete),
    ("/v1/admin/models", Verb::Get),
    ("/v1/admin/models", Verb::Post),
    ("/v1/admin/models/{id}", Verb::Patch),
    ("/v1/admin/models/{id}", Verb::Delete),
    ("/v1/fleets/bundles", Verb::Get),
    ("/v1/fleets/runners", Verb::Get),
    ("/v1/fleets/runners/{runner_id}", Verb::Get),
    ("/v1/fleets/runners/{runner_id}", Verb::Patch),
    ("/v1/fleets/runners/{runner_id}/events", Verb::Get),
    ("/v1/fleets/runners/{runner_id}/leases", Verb::Get),
    ("/v1/fleets/streams", Verb::Get),
];

fn actual() -> BTreeSet<Endpoint> {
    let admin = AdminRoute::ALL.iter().flat_map(|route| {
        route
            .verbs()
            .iter()
            .copied()
            .map(|verb| (route.meta().template, verb))
    });
    let bundles = TenantRoute::ALL.iter().flat_map(|route| {
        route
            .fleet_bundle_verbs()
            .into_iter()
            .flatten()
            .copied()
            .map(|verb| (route.meta().template, verb))
    });
    let operator = RunnerOpsRoute::ALL
        .iter()
        .filter(|route| **route != RunnerOpsRoute::Register)
        .flat_map(|route| {
            route
                .verbs()
                .iter()
                .copied()
                .map(|verb| (route.meta().template, verb))
        });

    admin.chain(bundles).chain(operator).collect()
}

#[test]
fn test_route_inventory_matches_interfaces() {
    let expected: BTreeSet<Endpoint> = EXPECTED.iter().copied().collect();
    let actual = actual();

    assert_eq!(
        actual, expected,
        "the M179 interface has an extra or missing route/verb pair"
    );
    assert_eq!(
        expected.len(),
        EXPECTED.len(),
        "the expected inventory repeats a route/verb pair"
    );
}
