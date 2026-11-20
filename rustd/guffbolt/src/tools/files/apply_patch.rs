//! `apply_patch`: Codex's patch grammar, applied through the environment's
//! file calls.
//!
//! The parser and the chunk matching are Codex's own, copied at a pinned
//! commit (`../../../vendor/apply_patch/NOTICE`). What is ours: the path gate on
//! every hunk, the reads and writes through the environment, and each file's
//! lines added and removed (`super::changes`). Every hunk is read and computed before the first
//! write, each from what the hunks before it leave, as Codex applies them one
//! after another: a patch with a hunk that does not apply changes nothing.
//! That check knows a file by the path a hunk spells, and a link inside the
//! working directory gives one file a second spelling, so an update lands by reading
//! its file again and applying its chunks to what is there, as Codex lands
//! each hunk. A write the environment refuses partway, or an update that no
//! longer applies to the file it lands on, is reported, and the hunks before
//! it stay landed. A hunk whose result is longer than one read carries is
//! refused as it is planned: landing re-reads a file whole, so a later hunk on
//! a file an earlier one grew past the cap would leave the patch half landed.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use bytes::Bytes;
use schemars::JsonSchema;
use serde::Deserialize;

use super::changes::{Change, report};
use super::{Answer, failed, inside, settled, too_long, whole};
use crate::tool::ToolOutput;
use crate::tools::env::{Env, MAX_READ_BYTES};
use crate::tools::process::env_of;
use crate::tools::{Context, Handler, code};

#[path = "../../../vendor/apply_patch/mod.rs"]
mod codex;

use self::codex::{Hunk, UpdateFileChunk};

/// How the answer starts: Codex's words, so a model trained on its harness
/// reads them unprompted.
const UPDATED: &str = "Success. Updated the following files:";
/// How each file is listed: added, modified, deleted.
const ADDED: char = 'A';
const MODIFIED: char = 'M';
const DELETED: char = 'D';
/// What a hunk on a path an earlier hunk moved away or deleted reads back
/// after the path.
const REMOVED_EARLIER: &str = "was moved or deleted earlier in this patch";

/// `apply_patch`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    /// The patch: `*** Begin Patch`, then hunks, then `*** End Patch`. A hunk
    /// is `*** Add File: <path>` with `+` lines, `*** Delete File: <path>`,
    /// or `*** Update File: <path>` with `@@ <context>` markers and ` `, `-`
    /// and `+` lines; `*** Move to: <path>` after an update renames the file.
    /// Paths are relative to the working directory.
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
    async fn text(&self, env: &dyn Env, path: &str) -> Result<String, ToolOutput> {
        match self.0.get(&key(path)) {
            Some(Some(text)) => Ok(text.clone()),
            Some(None) => Err(ToolOutput::failed(
                code::FILE_NOT_FOUND,
                &format!("{path} {REMOVED_EARLIER}"),
            )),
            None => whole(env, path).await,
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
#[derive(Debug, Clone, Copy)]
pub struct ApplyPatch;

impl Handler for ApplyPatch {
    const NAME: &'static str = "apply_patch";
    const DESCRIPTION: &'static str = "Edit files in the working directory with one patch in \
        apply_patch's grammar: add, update and delete hunks, each path relative to the \
        working directory. Every hunk is checked before any file changes; the answer lists each \
        file and ends with each file's lines added and removed.";
    type Arguments = Patch;

    async fn run(&self, arguments: Patch, context: Context<'_>) -> ToolOutput {
        settled(apply(&context, &arguments.patch).await)
    }
}

async fn apply(context: &Context<'_>, patch: &str) -> Answer {
    let env = env_of(context)?;
    let hunks = codex::parse_patch(patch).map_err(invalid)?;
    if hunks.is_empty() {
        return Err(invalid("the patch changes no file"));
    }
    let mut planned = Vec::with_capacity(hunks.len());
    let mut overlay = Overlay::default();
    for hunk in &hunks {
        planned.push(plan(env, &mut overlay, hunk).await?);
    }
    for step in &planned {
        land(env, step).await?;
    }
    Ok(ToolOutput::succeeded(summary(&planned)))
}

/// A patch that does not parse, or a hunk whose lines are not in its file.
fn invalid(why: impl std::fmt::Display) -> ToolOutput {
    ToolOutput::failed(code::PATCH_INVALID, &why.to_string())
}

/// `hunk` read and computed from what the hunks before it leave in
/// `overlay`, every path checked, nothing written.
async fn plan<'hunk>(
    env: &dyn Env,
    overlay: &mut Overlay,
    hunk: &'hunk Hunk,
) -> Result<Planned<'hunk>, ToolOutput> {
    match hunk {
        Hunk::AddFile { path, contents } => {
            let path = inside(env, named(path))?;
            fits(path, contents)?;
            overlay.leave(path, Some(contents.clone()));
            Ok(Planned::Add { path, contents })
        }
        Hunk::DeleteFile { path } => {
            let path = inside(env, named(path))?;
            let text = overlay.text(env, path).await?;
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
            let path = inside(env, named(path))?;
            // A move to the path the file already has is an update: landing
            // it as a move would delete what it just wrote.
            let moved_to = move_path
                .as_deref()
                .map(named)
                .map(|to| inside(env, to))
                .transpose()?
                .filter(|to| key(to) != key(path));
            let original = overlay.text(env, path).await?;
            let updated = codex::updated(path, &original, chunks).map_err(invalid)?;
            fits(moved_to.unwrap_or(path), &updated.contents)?;
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

/// Refuses a hunk leaving `text` at `path` when `whole` could not read it
/// back: `land` re-reads every update, so the cap on a read is the cap on a
/// write, and the refusal reads as a read's does.
fn fits(path: &str, text: &str) -> Result<(), ToolOutput> {
    if u64::try_from(text.len()).unwrap_or(u64::MAX) > MAX_READ_BYTES {
        return Err(too_long(path));
    }
    Ok(())
}

/// Lands one planned hunk. An update applies its chunks to its file as it is
/// now rather than writing the text planned for it: hunks through a link and
/// through its target were each planned from the file before either landed,
/// so the second's planned text would undo the first.
async fn land(env: &dyn Env, planned: &Planned<'_>) -> Result<(), ToolOutput> {
    match planned {
        Planned::Add { path, contents } => write(env, path, contents).await,
        Planned::Delete { path, .. } => delete(env, path).await,
        Planned::Update {
            path,
            moved_to,
            chunks,
            ..
        } => {
            let now = whole(env, path).await?;
            let updated = codex::updated(path, &now, chunks).map_err(invalid)?;
            // Planning held each spelling under the cap; two spellings of one
            // file, a link and its target, can still cross it together here.
            fits(moved_to.unwrap_or(path), &updated.contents)?;
            write(env, moved_to.unwrap_or(path), &updated.contents).await?;
            if moved_to.is_some() {
                delete(env, path).await
            } else {
                Ok(())
            }
        }
    }
}

/// Writes `text` to `path`; a refusal reads in the environment's words.
async fn write(env: &dyn Env, path: &str, text: &str) -> Result<(), ToolOutput> {
    env.write_file(path, Bytes::copy_from_slice(text.as_bytes()))
        .await
        .map_err(|failure| failed(&failure))
}

/// Removes `path`; a refusal reads in the environment's words.
async fn delete(env: &dyn Env, path: &str) -> Result<(), ToolOutput> {
    env.delete_file(path)
        .await
        .map_err(|failure| failed(&failure))
}

/// What the model reads back: Codex's summary, then each file's lines added
/// and removed, a file several hunks touched counted once.
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
    let mut changes: Vec<Change> = Vec::new();
    for hunk in planned {
        let (_mark, path) = hunk.listed();
        let (added, removed) = hunk.counts();
        let added = u64::try_from(added).unwrap_or(u64::MAX);
        let removed = u64::try_from(removed).unwrap_or(u64::MAX);
        if let Some(change) = changes.iter_mut().find(|change| change.path == path) {
            change.added = change.added.saturating_add(added);
            change.removed = change.removed.saturating_add(removed);
        } else {
            changes.push(Change {
                path: path.to_owned(),
                added,
                removed,
            });
        }
    }
    report(&text, &changes)
}

#[cfg(test)]
#[path = "apply_patch_chunk_tests.rs"]
mod chunk_tests;
