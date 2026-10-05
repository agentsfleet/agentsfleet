//! `apply_patch`: Codex's patch grammar, applied through the executor's file
//! calls.
//!
//! The parser and the chunk matching are Codex's own, copied at a pinned
//! commit (`../../vendor/apply_patch/NOTICE`). What is ours: the path gate on
//! every hunk, the reads and writes through the sandbox, and the count the
//! thread's cell shows. Every hunk is read and computed before the first
//! write, so a patch that cannot land leaves the workspace as it was.

use std::path::Path;

use afr_executor::Executor;
use bytes::Bytes;
use schemars::JsonSchema;
use serde::Deserialize;

use super::executor_of;
use super::files::{Answer, failed, inside, settled, whole};
use crate::catalog::{APPLY_PATCH, Entry};
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolErrorCode, ToolOutput};

#[path = "../../vendor/apply_patch/mod.rs"]
mod codex;

use self::codex::Hunk;

/// How the answer starts: Codex's words, so a model trained on its harness
/// reads them unprompted.
const UPDATED: &str = "Success. Updated the following files:";
/// How each file is listed: added, modified, deleted.
const ADDED: char = 'A';
const MODIFIED: char = 'M';
const DELETED: char = 'D';
/// The minus of the thread's `+N −M` cell.
const MINUS: char = '−';

/// `apply_patch`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Patch {
    /// The patch: `*** Begin Patch`, then hunks, then `*** End Patch`. A hunk
    /// is `*** Add File: <path>` with `+` lines, `*** Delete File: <path>`,
    /// or `*** Update File: <path>` with `@@ <context>` markers and ` `, `-`
    /// and `+` lines; `*** Move to: <path>` after an update renames the file.
    /// Paths are relative to the workspace.
    patch: String,
}

/// One hunk, read and computed, waiting to land.
#[derive(Debug)]
enum Planned<'hunk> {
    Add {
        path: &'hunk str,
        contents: &'hunk str,
    },
    Delete {
        path: &'hunk str,
        lines: usize,
    },
    Update {
        path: &'hunk str,
        moved_to: Option<&'hunk str>,
        contents: String,
        added: usize,
        removed: usize,
    },
}

impl Planned<'_> {
    /// How the summary lists this hunk: its mark, and the path it leaves.
    fn listed(&self) -> (char, &str) {
        match self {
            Self::Add { path, .. } => (ADDED, path),
            Self::Delete { path, .. } => (DELETED, path),
            Self::Update { path, moved_to, .. } => (MODIFIED, moved_to.unwrap_or(path)),
        }
    }

    /// The lines this hunk adds and removes.
    fn counts(&self) -> (usize, usize) {
        match self {
            Self::Add { contents, .. } => (contents.lines().count(), 0),
            Self::Delete { lines, .. } => (0, *lines),
            Self::Update { added, removed, .. } => (*added, *removed),
        }
    }
}

/// Applies a patch.
#[derive(Debug)]
pub(crate) struct ApplyPatch;

#[async_trait::async_trait]
impl Handler for ApplyPatch {
    const ENTRY: &'static Entry = &APPLY_PATCH;
    const DESCRIPTION: &'static str = "Edit files in the workspace with one patch in \
        apply_patch's grammar: add, update and delete hunks, each path relative to the \
        workspace. Every hunk is checked before any file changes; the answer lists each file \
        and ends with the lines added and removed.";
    type Arguments = Patch;

    async fn run(&self, arguments: Patch, context: ToolContext<'_, '_>) -> ToolOutput {
        settled(apply(&context, &arguments.patch).await)
    }
}

async fn apply(context: &ToolContext<'_, '_>, patch: &str) -> Answer {
    let executor = executor_of(context)?;
    let hunks = codex::parse_patch(patch).map_err(invalid)?;
    if hunks.is_empty() {
        return Err(invalid("the patch changes no file"));
    }
    let mut planned = Vec::with_capacity(hunks.len());
    for hunk in &hunks {
        planned.push(plan(executor, hunk).await?);
    }
    for step in &planned {
        land(executor, step).await?;
    }
    Ok(ToolOutput::succeeded(summary(&planned)))
}

/// A patch that does not parse, or a hunk whose lines are not in its file.
fn invalid(why: impl std::fmt::Display) -> ToolOutput {
    ToolOutput::failed(ToolErrorCode::PatchInvalid, &why.to_string())
}

/// `hunk` read and computed, every path checked, nothing written.
async fn plan<'hunk>(
    executor: &dyn Executor,
    hunk: &'hunk Hunk,
) -> Result<Planned<'hunk>, ToolOutput> {
    match hunk {
        Hunk::AddFile { path, contents } => Ok(Planned::Add {
            path: inside(named(path))?,
            contents,
        }),
        Hunk::DeleteFile { path } => {
            let path = inside(named(path))?;
            let text = whole(executor, path).await?;
            Ok(Planned::Delete {
                path,
                lines: text.lines().count(),
            })
        }
        Hunk::UpdateFile {
            path,
            move_path,
            chunks,
        } => {
            let path = inside(named(path))?;
            let moved_to = move_path.as_deref().map(named).map(inside).transpose()?;
            let original = whole(executor, path).await?;
            let updated = codex::updated(path, &original, chunks).map_err(invalid)?;
            Ok(Planned::Update {
                path,
                moved_to,
                contents: updated.contents,
                added: updated.added,
                removed: updated.removed,
            })
        }
    }
}

/// A hunk's path as text; the patch was text, so it always is.
fn named(path: &Path) -> &str {
    path.to_str().unwrap_or_default()
}

/// Lands one planned hunk.
async fn land(executor: &dyn Executor, planned: &Planned<'_>) -> Result<(), ToolOutput> {
    let landed = match planned {
        Planned::Add { path, contents } => {
            executor
                .write_file(path, Bytes::copy_from_slice(contents.as_bytes()))
                .await
        }
        Planned::Delete { path, .. } => executor.delete_file(path).await,
        Planned::Update {
            path,
            moved_to,
            contents,
            ..
        } => {
            let written = executor
                .write_file(
                    moved_to.unwrap_or(path),
                    Bytes::copy_from_slice(contents.as_bytes()),
                )
                .await;
            match (written, moved_to) {
                (Ok(()), Some(_moved)) => executor.delete_file(path).await,
                (outcome, _stayed) => outcome,
            }
        }
    };
    landed.map_err(|failure| failed(&failure))
}

/// What the model reads back: Codex's summary, then the count the thread's
/// cell shows on its own last line.
fn summary(planned: &[Planned<'_>]) -> String {
    let mut text = UPDATED.to_owned();
    for mark in [ADDED, MODIFIED, DELETED] {
        for (_mark, path) in planned
            .iter()
            .map(Planned::listed)
            .filter(|(listed, _path)| *listed == mark)
        {
            text.push('\n');
            text.push(mark);
            text.push(' ');
            text.push_str(path);
        }
    }
    let (added, removed) = planned
        .iter()
        .map(Planned::counts)
        .fold((0, 0), |(added, removed), (more, fewer)| {
            (added + more, removed + fewer)
        });
    format!("{text}\n+{added} {MINUS}{removed}")
}

#[cfg(test)]
#[path = "apply_patch/tests.rs"]
mod tests;
