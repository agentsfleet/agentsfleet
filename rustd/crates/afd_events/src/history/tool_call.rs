//! One tool call's kept record: everything it took and returned.
//!
//! The read behind "show all" under a call in the thread. The thread already
//! carries the call's id as `{fence}:{n}`, so the read is addressed by that
//! pair and answers one row. Scoped in the statement, as [`History::detail`]
//! is: a record of another workspace's fleet is not read and then hidden, it
//! is never read.

use afd_core::id::Uuid7;
use sqlx::Row as _;
use sqlx::postgres::PgRow;

use super::History;
use crate::error::{self, Error, Result, row_malformed};

/// What the read was doing, for the operator's log line.
const CONTEXT_TOOL_CALL: &str = "read one tool call's record";

/// One call by event, fence and number, inside its workspace and fleet.
///
/// `$1` workspace, `$2` fleet, `$3` event, `$4` fence, `$5` call number.
const SELECT_TOOL_CALL: &str = "\
SELECT arguments::text AS arguments, truncated_arguments, output,
       output_line_count, truncated
FROM core.fleet_tool_call_details
WHERE workspace_id = $1::uuid AND fleet_id = $2::uuid AND event_id = $3
  AND fencing_token = $4 AND call_number = $5";

/// Which call a read names: the two halves of its `{fence}:{n}` id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallAddress {
    /// The fencing token of the lease that made the call.
    pub fence: i64,
    /// The call's number in that lease's run, from 1.
    pub call_number: i64,
}

/// One kept record, as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ToolCallRow {
    /// The arguments, serialized to text.
    pub arguments: String,
    /// Whether the runner cut the arguments to fit.
    pub truncated_arguments: bool,
    /// Everything the call returned, as kept.
    pub output: String,
    /// How many lines the whole output had.
    pub output_line_count: i64,
    /// Whether the runner cut the output to fit.
    pub truncated: bool,
}

impl ToolCallRow {
    /// Decode one row, naming the column that refused.
    fn read(row: &PgRow) -> std::result::Result<Self, Error> {
        Ok(Self {
            arguments: row.try_get(0).map_err(row_malformed("arguments"))?,
            truncated_arguments: row
                .try_get(1)
                .map_err(row_malformed("truncated_arguments"))?,
            output: row.try_get(2).map_err(row_malformed("output"))?,
            output_line_count: row.try_get(3).map_err(row_malformed("output_line_count"))?,
            truncated: row.try_get(4).map_err(row_malformed("truncated"))?,
        })
    }
}

/// The row a suite outside this crate needs to construct.
#[cfg(feature = "test-util")]
impl ToolCallRow {
    /// A record whose output is `output`, with empty arguments.
    #[must_use]
    pub fn fixture(output: &str) -> Self {
        Self {
            arguments: "{}".to_owned(),
            truncated_arguments: false,
            output: output.to_owned(),
            output_line_count: i64::try_from(output.lines().count()).unwrap_or(i64::MAX),
            truncated: false,
        }
    }
}

impl History {
    /// One call's kept record, or nothing.
    ///
    /// Nothing for an unknown call, a call whose record was never posted, and
    /// a call of another workspace's fleet alike.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, or a row this build cannot
    /// read.
    pub async fn tool_call(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        event_id: &str,
        call: CallAddress,
    ) -> Result<Option<ToolCallRow>> {
        let mut connection = self.database.acquire().await?;
        let found = sqlx::query(SELECT_TOOL_CALL)
            .bind(workspace.as_str())
            .bind(fleet.as_str())
            .bind(event_id)
            .bind(call.fence)
            .bind(call.call_number)
            .fetch_optional(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_TOOL_CALL))?;
        found.as_ref().map(ToolCallRow::read).transpose()
    }
}
