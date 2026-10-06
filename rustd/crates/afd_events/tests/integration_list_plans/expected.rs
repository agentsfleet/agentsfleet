//! What each read text's generic plan must show, by the name `READ_TEXTS`
//! gives it.
//!
//! Every text: each bound it names is an index condition, and none is a
//! filter. A text with no actor filter must also walk its scope's index in
//! order with the bounds on that index, and sort nothing — the plan that lets
//! `LIMIT` stop the read at one page. An actor-PATTERN text makes no ordering
//! claim: the planner cannot know how much of the history a pattern matches,
//! and a pattern bound at run time is estimated as rare, so the planner may
//! read the matching rows and sort them. An EXACT actor walks the index that
//! names it, in order, as an unfiltered text walks its scope's.

/// The index a fleet-scoped read walks.
const FLEET_INDEX: &str = "idx_fleet_events_fleet_id_created_at_event_id";
/// The index a workspace-scoped read walks.
const WORKSPACE_INDEX: &str = "idx_fleet_events_workspace_id_created_at_event_id";
/// The index one actor's read in a fleet walks (`schema/930`).
const ACTOR_INDEX: &str = "idx_fleet_events_fleet_id_actor_created_at_event_id";

/// Each bound as Postgres prints it in a plan.
const FLEET: &str = "fleet_id = $2";
const WORKSPACE: &str = "workspace_id = $1";

/// `(name, the index an unfiltered text walks in order, its bounds)`.
pub(super) const EXPECTED: [(&str, Option<&str>, &[&str]); 12] = [
    (
        "fleet page",
        Some(FLEET_INDEX),
        &[FLEET, "created_at >= $3"],
    ),
    ("fleet page by actor", None, &[FLEET, "created_at >= $4"]),
    (
        "fleet page after",
        Some(FLEET_INDEX),
        &[
            FLEET,
            "created_at >= $5",
            "ROW(created_at, event_id) < ROW($3, $4)",
        ],
    ),
    (
        "fleet page after by actor",
        None,
        &[
            FLEET,
            "created_at >= $6",
            "ROW(created_at, event_id) < ROW($3, $4)",
        ],
    ),
    (
        "workspace page",
        Some(WORKSPACE_INDEX),
        &[WORKSPACE, "created_at >= $2"],
    ),
    (
        "workspace page by actor",
        None,
        &[WORKSPACE, "created_at >= $3"],
    ),
    (
        "workspace page after",
        Some(WORKSPACE_INDEX),
        &[
            WORKSPACE,
            "created_at >= $4",
            "ROW(created_at, event_id) < ROW($2, $3)",
        ],
    ),
    (
        "workspace page after by actor",
        None,
        &[
            WORKSPACE,
            "created_at >= $5",
            "ROW(created_at, event_id) < ROW($2, $3)",
        ],
    ),
    ("thread page", Some(FLEET_INDEX), &[FLEET]),
    (
        "thread page after",
        Some(FLEET_INDEX),
        &[FLEET, "ROW(created_at, event_id) < ROW($3, $4)"],
    ),
    (
        "fleet page of actor",
        Some(ACTOR_INDEX),
        &[FLEET, "actor = $3"],
    ),
    (
        "fleet page of actor after",
        Some(ACTOR_INDEX),
        &[
            FLEET,
            "actor = $5",
            "ROW(created_at, event_id) < ROW($3, $4)",
        ],
    ),
];
