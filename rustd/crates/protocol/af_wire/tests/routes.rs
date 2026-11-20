//! Every runner route, as the router, the published document and the runner
//! read it.
//!
//! The templates are composed from segments, so a typo in one segment moves
//! every route under it at once and nothing else in the build notices: the
//! router, the published document's attributes and the runner's client all
//! follow the same constant. This snapshot is where such a move shows up, as a
//! diff a reviewer reads.

use af_wire::paths;

/// Each template reads as the route a runner joins, one per line.
#[test]
fn test_route_templates_compose_from_their_segments() {
    let routes = [
        ("enrol", paths::ADMIN_RUNNERS),
        ("self", paths::RUNNER_SELF),
        ("stream", paths::RUNNER_STREAM),
        ("credentials_mint", paths::RUNNER_CREDENTIALS_MINT),
        ("memory", paths::RUNNER_MEMORY_FLEET),
        ("memory_recall", paths::RUNNER_MEMORY_RECALL),
        ("tool_calls", paths::RUN_ITEMS),
        ("schedules", paths::RUN_SCHEDULES),
        ("schedule", paths::RUN_SCHEDULE),
        ("schedule_runs", paths::RUN_SCHEDULE_RUNS),
        ("messages", paths::RUN_MESSAGES),
        ("task_inputs", paths::RUN_TASK_INPUTS),
        ("task_effects", paths::RUN_TASK_EFFECTS),
        ("task_effect", paths::RUN_TASK_EFFECT),
        ("task_decisions", paths::RUN_TASK_DECISIONS),
        ("task_decision", paths::RUN_TASK_DECISION),
    ];
    let rendered = routes
        .iter()
        .map(|(name, template)| format!("{name:<16} {template}"))
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(rendered, @r"
    enrol            /v1/admin/runners
    self             /v1/runners/me
    stream           /v1/runners/me/stream
    credentials_mint /v1/runners/me/credentials/mint
    memory           /v1/runners/me/memory/{fleet_id}
    memory_recall    /v1/runners/me/memory/{fleet_id}/recall
    tool_calls       /v1/runners/me/runs/{run_id}/items
    schedules        /v1/runners/me/runs/{run_id}/schedules
    schedule         /v1/runners/me/runs/{run_id}/schedules/{schedule_id}
    schedule_runs    /v1/runners/me/runs/{run_id}/schedules/{schedule_id}/runs
    messages         /v1/runners/me/runs/{run_id}/messages
    task_inputs      /v1/runners/me/runs/{run_id}/tasks/inputs
    task_effects     /v1/runners/me/runs/{run_id}/tasks/effects
    task_effect      /v1/runners/me/runs/{run_id}/tasks/effects/{effect_id}
    task_decisions   /v1/runners/me/runs/{run_id}/tasks/decisions
    task_decision    /v1/runners/me/runs/{run_id}/tasks/decisions/{call_id}
    ");
}

/// The runner presents the same segments the router matches: a held run's
/// tool-call records, joined the way the client joins it, match the template with the id
/// in place of its parameter.
#[test]
fn test_a_joined_run_route_matches_its_template() {
    let run = "019a0000-0000-7000-8000-000000000001";
    let joined = format!("{}/{run}/{}", paths::RUNNER_RUNS, paths::RUN_ITEMS_SUFFIX);
    assert_eq!(joined, paths::RUN_ITEMS.replace("{run_id}", run));
}

/// The three fleet-to-fleet routes sit under one held run, so a runner
/// reaches them only through a run it proves.
#[test]
fn runner_routes_snapshot_lists_the_three_fleet_routes() {
    let routes = [
        ("steers", paths::RUN_STEERS),
        ("fleets", paths::RUN_FLEETS),
        ("fleet_events", paths::RUN_FLEET_TURNS),
    ];
    let rendered = routes
        .iter()
        .map(|(name, template)| format!("{name:<12} {template}"))
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(rendered, @r"
    steers       /v1/runners/me/runs/{run_id}/steers
    fleets       /v1/runners/me/runs/{run_id}/fleets
    fleet_events /v1/runners/me/runs/{run_id}/fleets/{fleet_id}/turns
    ");
}
