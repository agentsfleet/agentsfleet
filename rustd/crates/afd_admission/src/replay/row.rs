//! The immutable saved admission, shared by background and in-request repair.

use jiff::Timestamp;

/// One admitted row awaiting its receipt, by the names
/// [`crate::sql::CLAIM_UNRECEIPTED`] and [`crate::sql::CLAIM_ADMISSION`]
/// give its columns.
///
/// `id` is the row's id and the event's: the entry is re-appended under the
/// same identity it was first answered with.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct Pending {
    pub(super) id: String,
    pub(super) fleet_id: String,
    pub(super) workspace_id: String,
    pub(super) actor: String,
    pub(super) event_type: String,
    pub(super) request_json: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub(super) event_created_at: Timestamp,
}

/// A [`Pending`] row, the receipt it already carries, and whether this call
/// claimed it — for a confirmation that appends only what is still owed.
#[derive(Debug, sqlx::FromRow)]
pub(super) struct Confirmable {
    #[sqlx(flatten)]
    pub(super) pending: Pending,
    pub(super) receipt: Option<String>,
    pub(super) claimed: bool,
}
