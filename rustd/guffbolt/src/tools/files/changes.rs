//! What an edit changed, reported at the end of its output.
//!
//! Each file's path and the lines it added and removed, so whoever records
//! the call reads the counts there rather than working them out from the
//! call's arguments.
//!
//! The report is the output's last block: a [`HEADER`] line, then one line
//! per file, the path as a JSON string so a path holding a line break stays
//! on its line. [`reported`] reads the block after the last header, so text
//! a path put in the output's earlier lines cannot add a file to it. A path's
//! control characters are written escaped, as `\n`, and a report line whose
//! path holds one is not a change, so no path read back can break a line of
//! whatever shows it.

use std::borrow::Cow;

use super::{ApplyPatch, FileAppend, FileEdit, FileEditHashed, FileWrite};
use crate::tools::Handler;
use crate::tools::env::{Env, MAX_READ_BYTES};

/// The line the report starts with.
pub const HEADER: &str = "Lines changed:";

/// The tools that edit files, each ending a successful answer with a report.
pub const EDITS: [&str; 5] = [
    FileWrite::NAME,
    FileAppend::NAME,
    FileEdit::NAME,
    FileEditHashed::NAME,
    ApplyPatch::NAME,
];

/// How many edits a line diff searches before it counts the lines between
/// the files' common start and end as all replaced: still a true diff, and
/// a cost bounded however far apart two long files are.
const MAX_EDITS: i64 = 1000;

/// One file an edit changed, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The file, as the call named it.
    pub path: String,
    /// Lines added.
    pub added: u64,
    /// Lines removed.
    pub removed: u64,
}

impl Change {
    /// `path` going from `before` to `after`, counted by a line diff.
    #[must_use]
    pub fn between(path: &str, before: &str, after: &str) -> Self {
        let (added, removed) = line_counts(before, after);
        Self {
            path: path.to_owned(),
            added,
            removed,
        }
    }

    /// The change's report line: the path, its control characters escaped,
    /// as JSON, then `+added -removed`.
    fn line(&self) -> String {
        let path = serde_json::Value::from(escaped(&self.path));
        format!("{path} +{} -{}", self.added, self.removed)
    }

    /// The change one report line names; none when the line is not one, or
    /// its path holds a control character, which no report writes.
    fn parse(line: &str) -> Option<Self> {
        let mut paths = serde_json::Deserializer::from_str(line).into_iter::<String>();
        let path = paths.next()?.ok()?;
        if path.contains(char::is_control) {
            return None;
        }
        let counts = line.get(paths.byte_offset()..)?.strip_prefix(" +")?;
        let (added, removed) = counts.split_once(" -")?;
        Some(Self {
            path,
            added: added.parse().ok()?,
            removed: removed.parse().ok()?,
        })
    }
}

/// `path` with each control character written as its escape, such as `\n`
/// for a line break.
fn escaped(path: &str) -> Cow<'_, str> {
    if !path.contains(char::is_control) {
        return Cow::Borrowed(path);
    }
    let escape = |character: char| -> String {
        if character.is_control() {
            character.escape_default().collect()
        } else {
            character.into()
        }
    };
    Cow::Owned(path.chars().map(escape).collect())
}

/// What appending `added` to `before`, the text at `path`, changes: the lines
/// `added` brings, and the last line of `before` it extends when that line
/// has no line break yet.
pub(crate) fn appended(path: &str, before: &str, added: &str) -> Change {
    let open = before.rsplit_once('\n').map_or(before, |(_, last)| last);
    Change::between(path, open, &format!("{open}{added}"))
}

/// `text` with the report of `changes` after it; `text` alone when nothing
/// changed.
#[must_use]
pub fn report(text: &str, changes: &[Change]) -> String {
    if changes.is_empty() {
        return text.to_owned();
    }
    let lines: Vec<String> = changes.iter().map(Change::line).collect();
    format!("{text}\n{HEADER}\n{}", lines.join("\n"))
}

/// The changes the report at the end of `output` names, in order; none when
/// the output carries no report, or its report holds a line that is not one.
#[must_use]
pub fn reported(output: &str) -> Vec<Change> {
    let marker = format!("\n{HEADER}\n");
    let Some((_, block)) = output.rsplit_once(&marker) else {
        return Vec::new();
    };
    block
        .lines()
        .map(Change::parse)
        .collect::<Option<Vec<Change>>>()
        .unwrap_or_default()
}

/// The text at `path` before a write replaces it: empty for a file that is
/// not there, none for one that cannot be read whole as text, whose lines
/// cannot be counted.
pub(crate) async fn before(env: &dyn Env, path: &str) -> Option<String> {
    match env.read_file(path, MAX_READ_BYTES).await {
        Ok(fetched) if !fetched.truncated => String::from_utf8(fetched.data.to_vec()).ok(),
        Ok(_longer) => None,
        Err(failure) if failure.is_not_found() => Some(String::new()),
        Err(_unread) => None,
    }
}

/// Lines `after` adds to `before` and lines it removes, by the shortest line
/// diff between them; past [`MAX_EDITS`] edits, the lines between their common
/// start and end count as all replaced.
fn line_counts(before: &str, after: &str) -> (u64, u64) {
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let head = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let (old, new) = (
        old.get(head..).unwrap_or_default(),
        new.get(head..).unwrap_or_default(),
    );
    let tail = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let old = old.get(..old.len() - tail).unwrap_or_default();
    let new = new.get(..new.len() - tail).unwrap_or_default();
    let common = shared_lines(old, new).unwrap_or_default();
    let count = |lines: usize| u64::try_from(lines - common).unwrap_or(u64::MAX);
    (count(new.len()), count(old.len()))
}

/// The most lines `old` and `new` share in order, by Myers' greedy search for
/// the shortest edit script; none past [`MAX_EDITS`] edits.
fn shared_lines<'a>(old: &[&'a str], new: &[&'a str]) -> Option<usize> {
    let (n, m) = (
        i64::try_from(old.len()).ok()?,
        i64::try_from(new.len()).ok()?,
    );
    let limit = (n + m).min(MAX_EDITS);
    let offset = limit + 1;
    let slot = |diagonal: i64| usize::try_from(diagonal + offset).ok();
    let line = |lines: &[&'a str], at: i64| -> Option<&'a str> {
        lines.get(usize::try_from(at).ok()?).copied()
    };
    // The furthest `x` each diagonal `x - y` reached.
    let mut furthest = vec![0_i64; usize::try_from(2 * limit + 3).ok()?];
    for edits in 0..=limit {
        for diagonal in (-edits..=edits).step_by(2) {
            let down = *furthest.get(slot(diagonal + 1)?)?;
            let right = *furthest.get(slot(diagonal - 1)?)?;
            let mut x = if diagonal == -edits || (diagonal != edits && right < down) {
                down
            } else {
                right + 1
            };
            let mut y = x - diagonal;
            while x < n && y < m && line(old, x) == line(new, y) {
                x += 1;
                y += 1;
            }
            *furthest.get_mut(slot(diagonal)?)? = x;
            if x >= n && y >= m {
                return usize::try_from((n + m - edits) / 2).ok();
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "changes_tests.rs"]
mod tests;
