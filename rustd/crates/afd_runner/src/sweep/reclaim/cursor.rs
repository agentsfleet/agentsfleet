//! Where a reclaim pass stopped, so the next one resumes rather than restarts.

use jiff::Timestamp;

/// The identifier every fleet's `(updated_at, id)` cursor starts below.
///
/// The nil UUID, which sorts below every version-7 identifier. Never stored —
/// only ever compared against.
pub(super) const CURSOR_START_ID: &str = "00000000-0000-0000-0000-000000000000";

/// The instant every fleet's cursor starts below.
///
/// The epoch: no fleet is stamped before the database's clock existed, so with
/// [`CURSOR_START_ID`] it sorts below every row. Never stored either.
const CURSOR_START_AT: Timestamp = Timestamp::UNIX_EPOCH;

/// Where a pass stopped, so the next one resumes rather than restarts.
///
/// An `Option` of the pair: `None` is the head of the population, which
/// [`CURSOR_START_AT`] and [`CURSOR_START_ID`] spell for the statement, and a
/// rewind is `= None` rather than several field assignments that have to agree.
#[derive(Debug, Clone, Default)]
pub(super) struct Cursor(pub(super) Option<(Timestamp, String)>);

impl Cursor {
    /// The instant this cursor resumes after.
    pub(super) const fn after_updated_at(&self) -> Timestamp {
        match &self.0 {
            Some((updated_at, _id)) => *updated_at,
            None => CURSOR_START_AT,
        }
    }

    /// The identifier this cursor resumes after.
    pub(super) fn after_id(&self) -> &str {
        match &self.0 {
            Some((_updated_at, id)) => id,
            None => CURSOR_START_ID,
        }
    }
}

/// `sql::sweep::SELECT_ACTIVE_FLEETS_AFTER`'s row.
#[derive(Debug, sqlx::FromRow)]
pub(super) struct ActiveFleet {
    pub(super) id: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub(super) updated_at: Timestamp,
}
