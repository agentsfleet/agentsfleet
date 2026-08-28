//! What a gate looks like coming out of the table, and what a decision reports.
//!
//! Split from [`super`] along the seam `afd_events::history::row` already
//! draws: the types and their decoders here, the verbs that run statements
//! there. The two change for different reasons — a column added moves this
//! file, a verb added moves that one — and holding them apart is what keeps
//! either at a size a reviewer reads in one pass.
//!
//! Both decoders read by INDEX, which is sound only because each is paired
//! with exactly one statement in [`crate::sql`]. Neither is shared.

use sqlx::Row as _;

use super::CONTEXT_RESOLVE;
use crate::decision::Decision;
use crate::{Result, error};

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
    pub gate_kind: String,
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
    pub created_at: i64,
    /// When it stops waiting.
    pub timeout_at: i64,
    /// When it was resolved, or `None` while pending.
    pub updated_at: Option<i64>,
    /// Who resolved it, empty while pending.
    pub resolved_by: String,
}

/// Where a page resumes.
///
/// The instant AND the id, because gates raised in the same millisecond are
/// ordinary — one run parks several tools at once — and an instant alone would
/// skip every sibling of the cursor row.
#[derive(Debug, Clone, Copy)]
pub struct Cursor<'a> {
    /// The last row's instant.
    pub created_at: i64,
    /// The last row's id, breaking the tie.
    pub gate_id: &'a str,
}

/// What a queue read is narrowed by.
#[derive(Debug, Clone, Copy, Default)]
pub struct Filter<'a> {
    /// Only gates at this status; pending when absent.
    pub status: Option<Decision>,
    /// Only gates raised by this fleet.
    pub fleet_id: Option<&'a str>,
    /// Only gates of this kind.
    pub gate_kind: Option<&'a str>,
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
    /// When it was resolved.
    pub updated_at: i64,
    /// Who resolved it.
    pub resolved_by: String,
    /// Their note.
    pub detail: String,
    /// The event the gate blocked, which a continuation resumes from.
    pub event_id: String,
    /// The continuation this decision landed, when it landed one.
    ///
    /// `Some` only for an approval THIS caller won: a denial continues
    /// nothing, and a caller who lost the race did not write the row the
    /// winner already did.
    pub continuation_event_id: Option<String>,
}

/// Reads one inbox row, through one error context.
pub(super) fn read_gate(row: &sqlx::postgres::PgRow, context: &'static str) -> Result<GateRow> {
    let unreadable = error::query(context);
    Ok(GateRow {
        gate_id: row.try_get(0).map_err(&unreadable)?,
        fleet_id: row.try_get(1).map_err(&unreadable)?,
        fleet_name: row.try_get(2).map_err(&unreadable)?,
        workspace_id: row.try_get(3).map_err(&unreadable)?,
        action_id: row.try_get(4).map_err(&unreadable)?,
        tool_name: row.try_get(5).map_err(&unreadable)?,
        action_name: row.try_get(6).map_err(&unreadable)?,
        gate_kind: row.try_get(7).map_err(&unreadable)?,
        proposed_action: row.try_get(8).map_err(&unreadable)?,
        evidence_json: row.try_get(9).map_err(&unreadable)?,
        blast_radius: row.try_get(10).map_err(&unreadable)?,
        status: row.try_get(11).map_err(&unreadable)?,
        detail: row.try_get(12).map_err(&unreadable)?,
        created_at: row.try_get(13).map_err(&unreadable)?,
        timeout_at: row.try_get(14).map_err(&unreadable)?,
        updated_at: row.try_get(15).map_err(&unreadable)?,
        resolved_by: row.try_get(16).map_err(&unreadable)?,
    })
}

/// Reads the attribution a resolve reports.
pub(super) fn read_resolved(row: &sqlx::postgres::PgRow) -> Result<Resolved> {
    let unreadable = error::query(CONTEXT_RESOLVE);
    Ok(Resolved {
        gate_id: row.try_get(0).map_err(&unreadable)?,
        action_id: row.try_get(1).map_err(&unreadable)?,
        workspace_id: row.try_get(2).map_err(&unreadable)?,
        fleet_id: row.try_get(3).map_err(&unreadable)?,
        status: row.try_get(4).map_err(&unreadable)?,
        updated_at: row.try_get(5).map_err(&unreadable)?,
        resolved_by: row.try_get(6).map_err(&unreadable)?,
        detail: row.try_get(7).map_err(&unreadable)?,
        event_id: row.try_get(8).map_err(&unreadable)?,
        continuation_event_id: None,
    })
}
