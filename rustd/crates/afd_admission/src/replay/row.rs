//! The immutable saved admission, shared by background and in-request repair.

use super::CONTEXT_REPLAY;
use crate::{Result, error::query, logical_id};
use sqlx::Row as _;

/// One admitted row awaiting its receipt.
pub(crate) struct Pending {
    pub(super) id: String,
    pub(super) fleet_id: String,
    pub(super) workspace_id: String,
    pub(super) actor: String,
    pub(super) event_type: String,
    pub(super) request_json: String,
    pub(super) event_created_at: i64,
    pub(super) logical_id: String,
}

impl Pending {
    /// Reads one [`sql::SELECT_UNRECEIPTED`] row.
    pub(crate) fn read(row: &sqlx::postgres::PgRow) -> Result<Self> {
        let text =
            |index: usize| -> Result<String> { row.try_get(index).map_err(query(CONTEXT_REPLAY)) };
        let number =
            |index: usize| -> Result<i64> { row.try_get(index).map_err(query(CONTEXT_REPLAY)) };
        Ok(Self {
            id: text(0)?,
            fleet_id: text(1)?,
            workspace_id: text(2)?,
            actor: text(3)?,
            event_type: text(4)?,
            request_json: text(5)?,
            event_created_at: number(6)?,
            logical_id: logical_id(number(7)?, number(8)?),
        })
    }
}
