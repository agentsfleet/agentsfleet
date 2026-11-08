//! What a gate looks like coming out of the table, and what a decision reports.
//!
//! Split from [`super`] along the seam `afd_events::history::row` already
//! draws: the types and their decoders here, the verbs that run statements
//! there. The two change for different reasons — a column added moves this
//! file, a verb added moves that one — and holding them apart is what keeps
//! either at a size a reviewer reads in one pass.
//!
//! Every decoder reads by column NAME through a `FromRow` row, so a
//! projection and its decoder cannot drift by position; the instants decode
//! through `jiff_sqlx`, which is why the rows are private and convert into the
//! public shapes below.

use afd_api_wire::approval::GateStatus;
use jiff::Timestamp;

/// One gate as the inbox shows it.
#[derive(Debug, Clone)]
pub struct GateRow {
    /// The gate's own row id — what a decision addresses it by.
    pub gate_id: String,
    /// The fleet that raised it.
    pub fleet_id: String,
    /// That fleet's name, joined rather than stored — see the statement.
    pub fleet_name: String,
    /// The workspace the gate belongs to.
    pub workspace_id: String,
    /// The action the gate is about.
    pub action_id: String,
    /// The tool asking.
    pub tool_name: String,
    /// The verb it wants.
    pub action_name: String,
    /// Which family of gate this is.
    pub gate_type: String,
    /// What the fleet proposes to do, in a person's words.
    pub proposed_action: String,
    /// The evidence behind the proposal, as stored JSON text.
    pub evidence_json: String,
    /// How far the consequences reach.
    pub blast_radius: String,
    /// Where the gate stands.
    pub status: String,
    /// The resolver's note, empty while pending.
    pub detail: String,
    /// When the gate was raised.
    pub created_at: Timestamp,
    /// When it stops waiting.
    pub expires_at: Timestamp,
    /// When it was resolved, or `None` while pending.
    pub resolved_at: Option<Timestamp>,
    /// Who resolved it, or `None` while pending.
    pub resolved_by: Option<String>,
    /// That person's name, from `identity.users`, or `None` when there is no
    /// person to name.
    ///
    /// Absent is ordinary and not an error: a pending gate has no decider, the
    /// sweeper is not a person, and a subject this deployment never saw sign up
    /// has no row. Every one of those renders as the shortened subject, which
    /// is what the column showed before a name was joined at all.
    pub resolved_by_name: Option<String>,
}

/// Where a page resumes.
///
/// The instant AND the id, because gates raised in the same millisecond are
/// ordinary — one run parks several tools at once — and an instant alone would
/// skip every sibling of the cursor row.
#[derive(Debug, Clone, Copy)]
pub struct Cursor<'a> {
    /// The last row's instant.
    pub created_at: Timestamp,
    /// The last row's id, breaking the tie.
    pub gate_id: &'a str,
}

/// What a queue read is narrowed by.
#[derive(Debug, Clone, Copy, Default)]
pub struct Filter<'a> {
    /// Only gates at this status; every status when absent.
    ///
    /// Absent used to mean `pending`, which made the status the one filter here
    /// that could not be turned off: an inbox wanting all five states had to ask
    /// five times. `?status=` is a filter, and omitting a filter returns
    /// everything — the same shape `fleet_id` and `gate_type` already have in
    /// this struct and in the predicate they share.
    ///
    /// [`GateStatus`] and not [`Decision`]: a filter names states a row can BE
    /// in, and the writer's vocabulary has three arms because two of the five
    /// states are not decisions anyone makes. Routing the filter through it
    /// left `auto_killed` unreachable.
    pub status: Option<GateStatus>,
    /// Only gates raised by this fleet.
    pub fleet_id: Option<&'a str>,
    /// Only gates of this type.
    pub gate_type: Option<&'a str>,
}

/// What answering a gate did.
///
/// Three outcomes and not two: "somebody already answered" is not a failure —
/// the gate IS resolved, which is what the operator wanted — but it is also not
/// this person's decision, and an inbox that reported it as theirs would
/// attribute an answer to the wrong human.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// This caller won the race; the row carries their attribution.
    Resolved(Resolved),
    /// An earlier writer had already terminated the row.
    AlreadyResolved(Resolved),
    /// No gate for that action, in that scope.
    NotFound,
}

/// The canonical attribution a resolve reports either way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The gate's row id.
    pub gate_id: String,
    /// The action it gated.
    pub action_id: String,
    /// The workspace it belonged to.
    pub workspace_id: String,
    /// The fleet that raised it.
    pub fleet_id: String,
    /// Where it now stands.
    pub status: String,
    /// When it was resolved, on the database clock.
    pub resolved_at: Timestamp,
    /// Who resolved it.
    pub resolved_by: String,
    /// Their note.
    pub detail: String,
    /// The event the gate blocked, which a continuation resumes from.
    ///
    /// `None` for a gate that held no run: the column is nullable for gates
    /// raised outside an event (an install-time integration grant), and a
    /// resolve of one must decode rather than fail after the row moved.
    pub event_id: Option<String>,
    /// The continuation this decision landed, when it landed one.
    ///
    /// `Some` only for an approval THIS caller won: a denial continues
    /// nothing, and a caller who lost the race did not write the row the
    /// winner already did.
    pub continuation_event_id: Option<String>,
}

/// One inbox row as the page and the by-id read return it.
#[derive(Debug, sqlx::FromRow)]
pub(super) struct StoredGate {
    gate_id: String,
    fleet_id: String,
    fleet_name: String,
    workspace_id: String,
    action_id: String,
    tool_name: String,
    action_name: String,
    gate_type: String,
    proposed_action: String,
    evidence_json: String,
    blast_radius: String,
    status: String,
    detail: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    expires_at: Timestamp,
    resolved_at: Option<jiff_sqlx::Timestamp>,
    resolved_by: Option<String>,
    resolved_by_name: Option<String>,
}

impl From<StoredGate> for GateRow {
    fn from(stored: StoredGate) -> Self {
        Self {
            gate_id: stored.gate_id,
            fleet_id: stored.fleet_id,
            fleet_name: stored.fleet_name,
            workspace_id: stored.workspace_id,
            action_id: stored.action_id,
            tool_name: stored.tool_name,
            action_name: stored.action_name,
            gate_type: stored.gate_type,
            proposed_action: stored.proposed_action,
            evidence_json: stored.evidence_json,
            blast_radius: stored.blast_radius,
            status: stored.status,
            detail: stored.detail,
            created_at: stored.created_at,
            expires_at: stored.expires_at,
            resolved_at: stored.resolved_at.map(jiff_sqlx::Timestamp::to_jiff),
            resolved_by: stored.resolved_by,
            resolved_by_name: stored.resolved_by_name,
        }
    }
}

/// The attribution a resolved gate reports, as both resolve reads return it.
#[derive(Debug, sqlx::FromRow)]
pub(super) struct StoredResolution {
    gate_id: String,
    action_id: String,
    workspace_id: String,
    fleet_id: String,
    status: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    resolved_at: Timestamp,
    resolved_by: String,
    detail: String,
    event_id: Option<String>,
}

impl From<StoredResolution> for Resolved {
    fn from(stored: StoredResolution) -> Self {
        Self {
            gate_id: stored.gate_id,
            action_id: stored.action_id,
            workspace_id: stored.workspace_id,
            fleet_id: stored.fleet_id,
            status: stored.status,
            resolved_at: stored.resolved_at,
            resolved_by: stored.resolved_by,
            detail: stored.detail,
            event_id: stored.event_id,
            continuation_event_id: None,
        }
    }
}

/// What the winning resolve returns: the attribution, and how many of the
/// fleet's gates still wait once it landed.
#[derive(Debug, sqlx::FromRow)]
pub(super) struct StoredWin {
    #[sqlx(flatten)]
    pub(super) resolution: StoredResolution,
    pub(super) pending_approvals: i64,
}
