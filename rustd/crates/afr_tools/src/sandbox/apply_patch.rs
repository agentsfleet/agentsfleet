//! `apply_patch`: Codex's patch grammar, applied through the executor's file
//! calls.
//!
//! The parser and the chunk matching are Codex's own, copied at a pinned
//! commit (`../../vendor/apply_patch/NOTICE`). What is ours: the path gate on
//! every hunk, the reads and writes through the sandbox, and the count the
//! thread's cell shows. Every hunk is read and computed before the first
//! write, each from what the hunks before it leave, as Codex applies them one
//! after another: a patch with a hunk that does not apply changes nothing.
//! That check knows a file by the path a hunk spells, and a link inside the
//! workspace gives one file a second spelling, so an update lands by reading
//! its file again and applying its chunks to what is there, as Codex lands
//! each hunk. A write the executor refuses partway, or an update that no
//! longer applies to the file it lands on, is reported, and the hunks before
//! it stay landed.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

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

use self::codex::{Hunk, UpdateFileChunk};

/// How the answer starts: Codex's words, so a model trained on its harness
/// reads them unprompted.
const UPDATED: &str = "Success. Updated the following files:";
/// How each file is listed: added, modified, deleted.
const ADDED: char = 'A';
const MODIFIED: char = 'M';
const DELETED: char = 'D';
/// The minus of the thread's `+N −M` cell.
const MINUS: char = '−';
/// What a hunk on a path an earlier hunk moved away or deleted reads back
/// after the path.
const REMOVED_EARLIER: &str = "was moved or deleted earlier in this patch";

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
        /// Applied again as the hunk lands, to the file as it is then.
        chunks: &'hunk [UpdateFileChunk],
        added: usize,
        removed: usize,
    },
}

/// What the hunks planned so far leave at each path they touched, by its
/// spelling with `./` dropped: the text, or `None` where one moved the file
/// away or deleted it. A later hunk on that path starts from here.
#[derive(Debug, Default)]
struct Overlay(BTreeMap<PathBuf, Option<String>>);

impl Overlay {
    /// The text at `path`: what an earlier hunk left, or the file itself.
    async fn text(&self, executor: &dyn Executor, path: &str) -> Result<String, ToolOutput> {
        match self.0.get(&key(path)) {
            Some(Some(text)) => Ok(text.clone()),
            Some(None) => Err(ToolOutput::failed(
                ToolErrorCode::FileNotFound,
                &format!("{path} {REMOVED_EARLIER}"),
            )),
            None => whole(executor, path).await,
        }
    }

    /// Records what `path` holds once the hunk being planned lands.
    fn leave(&mut self, path: &str, text: Option<String>) {
        self.0.insert(key(path), text);
    }

    /// Records `path` updated to `text`, and moved to `moved_to` when the
    /// hunk names a new path.
    fn update(&mut self, path: &str, moved_to: Option<&str>, text: &str) {
        if moved_to.is_some() {
            self.leave(path, None);
        }
        self.leave(moved_to.unwrap_or(path), Some(text.to_owned()));
    }
}

/// `path` as the overlay keys it: `src/a`, `./src/a` and `src/./a` are one
/// file.
fn key(path: &str) -> PathBuf {
    Path::new(path)
        .components()
        .filter(|part| *part != Component::CurDir)
        .collect()
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
    let mut overlay = Overlay::default();
    for hunk in &hunks {
        planned.push(plan(executor, &mut overlay, hunk).await?);
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

/// `hunk` read and computed from what the hunks before it leave in
/// `overlay`, every path checked, nothing written.
async fn plan<'hunk>(
    executor: &dyn Executor,
    overlay: &mut Overlay,
    hunk: &'hunk Hunk,
) -> Result<Planned<'hunk>, ToolOutput> {
    match hunk {
        Hunk::AddFile { path, contents } => {
            let path = inside(named(path))?;
            overlay.leave(path, Some(contents.clone()));
            Ok(Planned::Add { path, contents })
        }
        Hunk::DeleteFile { path } => {
            let path = inside(named(path))?;
            let text = overlay.text(executor, path).await?;
            overlay.leave(path, None);
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
            // A move to the path the file already has is an update: landing
            // it as a move would delete what it just wrote.
            let moved_to = move_path
                .as_deref()
                .map(named)
                .map(inside)
                .transpose()?
                .filter(|to| key(to) != key(path));
            let original = overlay.text(executor, path).await?;
            let updated = codex::updated(path, &original, chunks).map_err(invalid)?;
            overlay.update(path, moved_to, &updated.contents);
            Ok(Planned::Update {
                path,
                moved_to,
                chunks,
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

/// Lands one planned hunk. An update applies its chunks to its file as it is
/// now rather than writing the text planned for it: hunks through a link and
/// through its target were each planned from the file before either landed,
/// so the second's planned text would undo the first.
async fn land(executor: &dyn Executor, planned: &Planned<'_>) -> Result<(), ToolOutput> {
    match planned {
        Planned::Add { path, contents } => write(executor, path, contents).await,
        Planned::Delete { path, .. } => delete(executor, path).await,
        Planned::Update {
            path,
            moved_to,
            chunks,
            ..
        } => {
            let now = whole(executor, path).await?;
            let updated = codex::updated(path, &now, chunks).map_err(invalid)?;
            write(executor, moved_to.unwrap_or(path), &updated.contents).await?;
            if moved_to.is_some() {
                delete(executor, path).await
            } else {
                Ok(())
            }
        }
    }
}

/// Writes `text` to `path`; a refusal reads in the executor's words.
async fn write(executor: &dyn Executor, path: &str, text: &str) -> Result<(), ToolOutput> {
    executor
        .write_file(path, Bytes::copy_from_slice(text.as_bytes()))
        .await
        .map_err(|failure| failed(&failure))
}

/// Removes `path`; a refusal reads in the executor's words.
async fn delete(executor: &dyn Executor, path: &str) -> Result<(), ToolOutput> {
    executor
        .delete_file(path)
        .await
        .map_err(|failure| failed(&failure))
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

#[cfg(test)]
#[path = "apply_patch/landing_tests.rs"]
mod landing_tests;

#[cfg(test)]
#[path = "apply_patch/chunk_tests.rs"]
mod chunk_tests;

#[cfg(test)]
#[path = "apply_patch/link_tests.rs"]
mod link_tests;
