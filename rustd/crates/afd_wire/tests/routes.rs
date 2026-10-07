//! Every runner route, as the router, the published document and the runner
//! read it.
//!
//! The templates are composed from segments, so a typo in one segment moves
//! every route under it at once and nothing else in the build notices: the
//! router, the published document's attributes and the runner's client all
//! follow the same constant. This snapshot is where such a move shows up, as a
//! diff a reviewer reads.

use std::fmt::Write as _;

use afd_wire::paths;

/// Each template reads as the route a runner joins, one per line.
#[test]
fn test_route_templates_compose_from_their_segments() {
    let routes = [
        ("enrol", paths::RUNNERS),
        ("self", paths::RUNNER_SELF),
        ("heartbeats", paths::RUNNER_HEARTBEATS),
        ("leases", paths::RUNNER_LEASES),
        ("reports", paths::RUNNER_REPORTS),
        ("credentials_mint", paths::RUNNER_CREDENTIALS_MINT),
        ("memory", paths::RUNNER_MEMORY_FLEET),
        ("memory_recall", paths::RUNNER_MEMORY_RECALL),
        ("bundle", paths::RUNNER_BUNDLE),
        ("activity", paths::LEASE_ACTIVITY),
        ("renew", paths::LEASE_RENEW),
        ("tool_calls", paths::LEASE_TOOL_CALLS),
        ("schedules", paths::LEASE_SCHEDULES),
        ("schedule", paths::LEASE_SCHEDULE),
        ("schedule_runs", paths::LEASE_SCHEDULE_RUNS),
        ("messages", paths::LEASE_MESSAGES),
    ];
    let rendered = routes
        .iter()
        .fold(String::new(), |mut out, (name, template)| {
            // Writing to a `String` cannot fail.
            let _ = writeln!(out, "{name:<16} {template}");
            out
        });
    insta::assert_snapshot!(rendered, @r"
    enrol            /v1/runners
    self             /v1/runners/me
    heartbeats       /v1/runners/me/heartbeats
    leases           /v1/runners/me/leases
    reports          /v1/runners/me/reports
    credentials_mint /v1/runners/me/credentials/mint
    memory           /v1/runners/me/memory/{fleet_id}
    memory_recall    /v1/runners/me/memory/{fleet_id}/recall
    bundle           /v1/runners/me/bundles/{content_hash}
    activity         /v1/runners/me/leases/{lease_id}/activity
    renew            /v1/runners/me/leases/{lease_id}/renew
    tool_calls       /v1/runners/me/leases/{lease_id}/tool-calls
    schedules        /v1/runners/me/leases/{lease_id}/schedules
    schedule         /v1/runners/me/leases/{lease_id}/schedules/{schedule_id}
    schedule_runs    /v1/runners/me/leases/{lease_id}/schedules/{schedule_id}/runs
    messages         /v1/runners/me/leases/{lease_id}/messages
    ");
}

/// The runner presents the same segments the router matches: a held lease's
/// activity, joined the way the client joins it, is the template with the id
/// in place of its parameter.
#[test]
fn test_a_joined_lease_route_matches_its_template() {
    let lease_id = "019a0000-0000-7000-8000-000000000001";
    let joined = format!(
        "{}/{lease_id}/{}",
        paths::RUNNER_LEASES,
        paths::LEASE_ACTIVITY_SUFFIX
    );
    assert_eq!(
        joined,
        paths::LEASE_ACTIVITY.replace("{lease_id}", lease_id)
    );
}
