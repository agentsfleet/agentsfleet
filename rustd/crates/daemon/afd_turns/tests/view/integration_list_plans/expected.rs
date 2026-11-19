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
const FLEET_INDEX: &str = "idx_turns_thread_id_created_at_id";
/// The index a workspace-scoped read walks.
const WORKSPACE_INDEX: &str = "idx_turns_workspace_id_created_at_id";
/// The index one actor's read in a fleet walks (`schema/20261108125100_thread_turns.sql`).
const ACTOR_INDEX: &str = "idx_turns_thread_id_actor_created_at_id";
/// Equality-filtered event types keep the cursor on the same ordered index.
const TYPE_INDEX: &str = "idx_turns_thread_id_kind_created_at_id";

/// Each bound as Postgres prints it in a plan.
const FLEET: &str = "thread_id = $2";
const WORKSPACE: &str = "workspace_id = $1";

/// `(name, the index an unfiltered text walks in order, its bounds)`.
pub(super) const EXPECTED: [(&str, Option<&str>, &[&str]); 15] = [
    (
        "fleet page",
        Some(FLEET_INDEX),
        &[FLEET, "created_at >= COALESCE($3"],
    ),
    (
        "fleet page by actor",
        None,
        &[FLEET, "created_at >= COALESCE($4"],
    ),
    (
        "fleet page after",
        Some(FLEET_INDEX),
        &[
            FLEET,
            "created_at >= COALESCE($5",
            "ROW(created_at, id) < ROW($3, $4)",
        ],
    ),
    (
        "fleet page after by actor",
        None,
        &[
            FLEET,
            "created_at >= COALESCE($6",
            "ROW(created_at, id) < ROW($3, $4)",
        ],
    ),
    (
        "workspace page",
        Some(WORKSPACE_INDEX),
        &[WORKSPACE, "created_at >= COALESCE($2"],
    ),
    (
        "workspace page by actor",
        None,
        &[WORKSPACE, "created_at >= COALESCE($3"],
    ),
    (
        "workspace page after",
        Some(WORKSPACE_INDEX),
        &[
            WORKSPACE,
            "created_at >= COALESCE($4",
            "ROW(created_at, id) < ROW($2, $3)",
        ],
    ),
    (
        "workspace page after by actor",
        None,
        &[
            WORKSPACE,
            "created_at >= COALESCE($5",
            "ROW(created_at, id) < ROW($2, $3)",
        ],
    ),
    ("thread page", Some(FLEET_INDEX), &[FLEET]),
    ("thread filtered", Some(TYPE_INDEX), &[FLEET, "kind = $3"]),
    (
        "thread filtered after",
        Some(TYPE_INDEX),
        &[FLEET, "kind = $3", "ROW(created_at, id) < ROW($4, $5)"],
    ),
    (
        "thread page after",
        Some(FLEET_INDEX),
        &[FLEET, "ROW(created_at, id) < ROW($3, $4)"],
    ),
    (
        "thread finished before",
        Some(FLEET_INDEX),
        &[FLEET, "ROW(created_at, id) < ROW($3, $4)"],
    ),
    (
        "fleet page of actor",
        Some(ACTOR_INDEX),
        &[FLEET, "actor = $3"],
    ),
    (
        "fleet page of actor after",
        Some(ACTOR_INDEX),
        &[FLEET, "actor = $5", "ROW(created_at, id) < ROW($3, $4)"],
    ),
];
