//! One tool call's kept record: everything it took and returned.
//!
//! The read behind "show all" under a call in the thread. The thread already
//! carries the call's id as `{fence}:{n}`, so the read is addressed by that
//! pair and answers one row. Scoped in the statement, as [`History::detail`]
//! is: a record of another workspace's fleet is not read and then hidden, it
//! is never read.

use af_core::id::Uuid7;
use af_wire::tool_detail::{ItemKind, ItemStatus};

use super::History;
use crate::view::error::{self, Result};

/// What the read was doing, for the operator's log line.
const CONTEXT_TOOL_CALL: &str = "read one tool call's record";

/// One call by turn, fence and number, inside its workspace and fleet.
///
/// The turn is the one the call's run was assigned for, which the thread
/// shows the call under; the record itself names the turn newest when its
/// step began. The call's number in its run is the record's `sequence`.
///
/// `$1` workspace, `$2` fleet, `$3` turn, `$4` fence, `$5` call number.
const SELECT_TOOL_CALL: &str = include_str!("select_tool_call.sql");

/// Which call a read names: the two halves of its `{fence}:{n}` id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallAddress {
    /// The fencing token of the run that made the call.
    pub fence: i64,
    /// The call's number in that run's run, from 1.
    pub call_number: i64,
}

/// The row as the statement selects it, its words still text.
#[derive(sqlx::FromRow)]
struct StoredToolCall {
    kind: String,
    tool: Option<String>,
    status: String,
    arguments: String,
    truncated_arguments: bool,
    output: String,
    output_line_count: i64,
    truncated: bool,
    changes: Option<String>,
}

/// One kept record, its words read.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ToolCallRow {
    /// What kind of thing the call was.
    pub kind: ItemKind,
    /// The tool it called; none for an item that called no tool.
    pub tool: Option<String>,
    /// How it ended.
    pub status: ItemStatus,
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
    /// For an edit, each file it changed and the lines it added and removed,
    /// as the wire spells the list; none for any other call.
    pub changes: Option<String>,
}

impl ToolCallRow {
    /// The record `stored` holds, or the decode error of a word this build
    /// does not spell.
    fn read(stored: StoredToolCall) -> sqlx::Result<Self> {
        Ok(Self {
            kind: word("kind", &stored.kind, ItemKind::parse)?,
            tool: stored.tool,
            status: word("status", &stored.status, ItemStatus::parse)?,
            arguments: stored.arguments,
            truncated_arguments: stored.truncated_arguments,
            output: stored.output,
            output_line_count: stored.output_line_count,
            truncated: stored.truncated,
            changes: stored.changes,
        })
    }
}

/// The word `stored` names, or the error sqlx raises for a column it cannot
/// decode: the record verb writes only the words this build spells.
fn word<T>(column: &str, stored: &str, parse: fn(&str) -> Option<T>) -> sqlx::Result<T> {
    parse(stored).ok_or_else(|| sqlx::Error::ColumnDecode {
        index: column.to_owned(),
        source: format!("{stored:?} is no word this build reads").into(),
    })
}

/// The row a suite outside this crate needs to construct.
#[cfg(feature = "test-util")]
impl ToolCallRow {
    /// A `shell` call that succeeded, whose output is `output`, with empty
    /// arguments.
    #[must_use]
    pub fn fixture(output: &str) -> Self {
        Self {
            kind: ItemKind::ToolCall,
            tool: Some("shell".to_owned()),
            status: ItemStatus::Succeeded,
            arguments: "{}".to_owned(),
            truncated_arguments: false,
            output: output.to_owned(),
            output_line_count: i64::try_from(output.lines().count()).unwrap_or(i64::MAX),
            truncated: false,
            changes: None,
        }
    }

    /// The same record, a `file_edit` whose `changes` column holds `changes`.
    #[must_use]
    pub fn with_changes(self, changes: &str) -> Self {
        Self {
            kind: ItemKind::Edit,
            tool: Some("file_edit".to_owned()),
            changes: Some(changes.to_owned()),
            ..self
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
        turn_id: &Uuid7,
        call: CallAddress,
    ) -> Result<Option<ToolCallRow>> {
        let mut connection = self.database.acquire().await?;
        sqlx::query_as::<_, StoredToolCall>(SELECT_TOOL_CALL)
            .bind(workspace)
            .bind(fleet)
            .bind(turn_id)
            .bind(call.fence)
            .bind(call.call_number)
            .fetch_optional(&mut *connection)
            .await
            .and_then(|found| found.map(ToolCallRow::read).transpose())
            .map_err(error::query(CONTEXT_TOOL_CALL))
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::*;

    /// A stored edit, its words spelled `kind` and `status`.
    fn stored(kind: &str, status: &str) -> StoredToolCall {
        StoredToolCall {
            kind: kind.to_owned(),
            tool: Some("file_edit".to_owned()),
            status: status.to_owned(),
            arguments: "{}".to_owned(),
            truncated_arguments: false,
            output: "Success.".to_owned(),
            output_line_count: 1,
            truncated: false,
            changes: None,
        }
    }

    #[test]
    fn a_stored_record_reads_its_kind_tool_and_status() {
        let row = ToolCallRow::read(stored("edit", "failed")).expect("words this build spells");
        assert_eq!(row.kind, ItemKind::Edit);
        assert_eq!(row.tool.as_deref(), Some("file_edit"));
        assert_eq!(row.status, ItemStatus::Failed);
    }

    #[test]
    fn a_word_this_build_does_not_spell_is_a_column_it_cannot_decode() {
        for (record, column) in [
            (stored("integration", "succeeded"), "kind"),
            (stored("edit", "running"), "status"),
        ] {
            let refused = ToolCallRow::read(record).expect_err("an unknown word");
            assert!(
                matches!(&refused, sqlx::Error::ColumnDecode { index, .. } if index == column),
                "{refused:?}"
            );
        }
    }
}
