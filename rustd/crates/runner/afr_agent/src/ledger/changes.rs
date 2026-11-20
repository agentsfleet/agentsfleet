//! What kind of item a call is, and, for an edit, the files it changed, read
//! from the report `guffbolt`'s file tools end a successful answer with. No
//! count is worked out from the call's arguments.

use std::borrow::Cow;

use af_wire::tool_detail::{ItemKind, ItemStatus};
use af_wire::tool_trace::{CHANGES_MAX, FileChange, ToolCallStatus, changes_fit};
use guffbolt::tools::files::changes::{EDITS, reported};

/// An edit's report broke a bound its changes are carried under, so the call
/// is recorded without them.
pub(super) const EVENT_CHANGES_DROPPED: &str = "tool_changes_dropped";

/// The kind of item a call to `tool` is.
pub(super) fn kind(tool: &str) -> ItemKind {
    if EDITS.contains(&tool) {
        ItemKind::Edit
    } else {
        ItemKind::ToolCall
    }
}

/// How a call that answered ended, as its item says it. A record is written
/// only for a call that answered, so an interrupted one never reaches here;
/// it reads as failed should it.
pub(super) const fn item_status(status: ToolCallStatus) -> ItemStatus {
    match status {
        ToolCallStatus::Succeeded => ItemStatus::Succeeded,
        ToolCallStatus::Failed | ToolCallStatus::Interrupted => ItemStatus::Failed,
    }
}

/// The files a call of `kind` that ended `status` changed, read from its
/// scrubbed `output`: none for a call that is not an edit or did not
/// succeed, and none, logged under `run_id`, when the report breaks a bound
/// the record, the trace row and the frame carry changes under.
pub(super) fn changed(
    run_id: &str,
    kind: ItemKind,
    status: ToolCallStatus,
    output: &str,
) -> Vec<FileChange<'static>> {
    if kind != ItemKind::Edit || status != ToolCallStatus::Succeeded {
        return Vec::new();
    }
    let changes: Vec<FileChange<'static>> = reported(output)
        .into_iter()
        .map(|change| FileChange {
            path: Cow::Owned(change.path),
            added: change.added,
            removed: change.removed,
        })
        .collect();
    if changes_fit(&changes) {
        return changes;
    }
    let files = changes.len();
    let limit = CHANGES_MAX;
    let event = EVENT_CHANGES_DROPPED;
    tracing::warn!(run_id, files, limit, event);
    Vec::new()
}

#[cfg(test)]
#[path = "changes_tests.rs"]
mod tests;
