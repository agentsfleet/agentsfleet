//! `file_read_hashed` and `file_edit_hashed`: Hashline, as nullclaw's tools
//! of the same names do it (`oss/zig/nullclaw/src/tools/file_read_hashed.zig`
//! and `file_edit_hashed.zig`), because those are the tools the published
//! page names and the Zig runner wired, and a tag read under one runner must
//! edit under the other.
//!
//! A read tags every line `L<n>:<hhh>|<line>`: three hex digits of a hash
//! over the line and the one before it, so a tag names a line by its text and
//! its neighbour, not its number alone. An edit names a line, or a range, by
//! tag. The tag is looked for within a radius of the line it was read at, so
//! an edit lands after lines moved above it, and is refused when the line
//! changed or the tag matches twice. A stale read never lands.

use std::fmt::Write as _;

use bytes::Bytes;
use schemars::JsonSchema;
use serde::Deserialize;

use super::executor_of;
use super::files::{Answer, failed, inside, settled, whole};
use crate::catalog::{Entry, FILE_EDIT_HASHED, FILE_READ_HASHED};
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolErrorCode, ToolOutput};

/// Fowler–Noll–Vo 1a, 32 bits: nullclaw's hash, so a tag reads the same
/// under either runner.
const FNV_OFFSET: u32 = 0x811c_9dc5;
const FNV_PRIME: u32 = 0x0100_0193;
/// The bits of the hash a tag keeps: twelve, as three hex digits.
const TAG_BITS: u32 = 0xfff;
/// How many hex digits that is.
const TAG_LEN: usize = 3;
/// What is hashed between the line before and the line.
const BETWEEN: &str = "|";
/// What the hash ignores at either end of a line.
const TRIMMED: &[char] = &[' ', '\t', '\r', '\n'];
/// How a tag starts, and what parts its line number from its hash.
const TAG_START: char = 'L';
const TAG_SEPARATOR: char = ':';
/// What parts a tag from its line in a read.
const TAG_END: char = '|';
/// How far either side of its read line a tag is looked for: nullclaw's
/// `RADIUS`.
const RADIUS: usize = 50;
/// The argument an edit's start and end tags are named by.
const TARGET: &str = "target";
const END_TARGET: &str = "end_target";

/// `file_read_hashed`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Named {
    /// The file, relative to the workspace.
    path: String,
}

/// `file_edit_hashed`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Range {
    /// The file, relative to the workspace.
    path: String,
    /// The tag of the line to replace, as `file_read_hashed` printed it:
    /// `L10:abc`.
    target: String,
    /// The tag of the last line to replace, for a range: `L15:def`. The one
    /// line named by `target` when absent.
    #[serde(default)]
    end_target: Option<String>,
    /// What replaces the line or range.
    new_text: String,
}

/// Reads a file with every line tagged.
#[derive(Debug)]
pub(crate) struct FileReadHashed;

#[async_trait::async_trait]
impl Handler for FileReadHashed {
    const ENTRY: &'static Entry = &FILE_READ_HASHED;
    const DESCRIPTION: &'static str = "Read a file in the workspace with every line tagged \
        L<line>:<hash>|<text>, for file_edit_hashed to name lines by. The file must be text \
        and fit one read.";
    type Arguments = Named;

    async fn run(&self, arguments: Named, context: ToolContext<'_, '_>) -> ToolOutput {
        settled(read(&context, &arguments.path).await)
    }
}

async fn read(context: &ToolContext<'_, '_>, path: &str) -> Answer {
    let executor = executor_of(context)?;
    let path = inside(path)?;
    let text = whole(executor, path).await?;
    Ok(ToolOutput::succeeded(tagged(&text)))
}

/// Replaces a line, or a range of lines, named by tag.
#[derive(Debug)]
pub(crate) struct FileEditHashed;

#[async_trait::async_trait]
impl Handler for FileEditHashed {
    const ENTRY: &'static Entry = &FILE_EDIT_HASHED;
    const DESCRIPTION: &'static str = "Replace one line, or the range from target to \
        end_target, in a file in the workspace, naming the lines by the tags \
        file_read_hashed printed. A tag the file no longer matches is refused: read the \
        file again.";
    type Arguments = Range;

    async fn run(&self, arguments: Range, context: ToolContext<'_, '_>) -> ToolOutput {
        settled(edit(&context, &arguments).await)
    }
}

async fn edit(context: &ToolContext<'_, '_>, arguments: &Range) -> Answer {
    let executor = executor_of(context)?;
    let path = inside(&arguments.path)?;
    let start = Target::parse(&arguments.target)?;
    let end = arguments
        .end_target
        .as_deref()
        .map(Target::parse)
        .transpose()?;
    let text = whole(executor, path).await?;
    let lines = lines(&text);
    let from = start.locate(&lines, start.line.saturating_sub(1), TARGET, path)?;
    let to = match end {
        None => from,
        Some(end) => {
            if end.line < start.line {
                return Err(mismatch(&format!(
                    "{END_TARGET} L{} comes before {TARGET} L{}",
                    end.line, start.line
                )));
            }
            // Looked for where it sits after the start moved by as much as it did.
            let hint = (end.line.saturating_sub(1) + from)
                .saturating_sub(start.line.saturating_sub(1))
                .min(lines.len().saturating_sub(1));
            let to = end.locate(&lines, hint, END_TARGET, path)?;
            if to < from {
                return Err(mismatch(&format!(
                    "{END_TARGET} {} was found before {TARGET} {}",
                    arguments.target,
                    arguments.end_target.as_deref().unwrap_or_default()
                )));
            }
            to
        }
    };
    let edited = spliced(&text, &lines, from, to, &arguments.new_text);
    executor
        .write_file(path, Bytes::from(edited))
        .await
        .map_err(|failure| failed(&failure))?;
    Ok(ToolOutput::succeeded(format!(
        "Edited {path}: {} was line {}, {} lines replaced",
        arguments.target,
        from + 1,
        to - from + 1
    )))
}

/// `text` with lines `from..=to` replaced by `new_text`, which gets a newline
/// after it when lines follow and it ends without one, as nullclaw splices.
fn spliced(text: &str, lines: &[Line<'_>], from: usize, to: usize, new_text: &str) -> String {
    let before_at = lines.get(from).map_or(0, |line| line.start);
    let after_at = lines
        .get(to.saturating_add(1))
        .map_or(text.len(), |line| line.start);
    let (before, _replaced) = text.split_at(before_at);
    let (_replaced, after) = text.split_at(after_at);
    let separator = if !after.is_empty() && !new_text.is_empty() && !new_text.ends_with('\n') {
        "\n"
    } else {
        ""
    };
    [before, new_text, separator, after].concat()
}

/// A failed tag lookup, with `detail` for the model.
fn mismatch(detail: &str) -> ToolOutput {
    ToolOutput::failed(ToolErrorCode::HashMismatch, detail)
}

/// One line of the file: where it starts, and its text without its newline.
#[derive(Debug, Clone, Copy)]
struct Line<'text> {
    start: usize,
    text: &'text str,
}

/// `text`'s lines as nullclaw collects them: split on `\n`, so a file that
/// ends in one ends in an empty line, and tags match across runners.
fn lines(text: &str) -> Vec<Line<'_>> {
    let mut start = 0;
    text.split('\n')
        .map(|segment| {
            let line = Line {
                start,
                text: segment,
            };
            start += segment.len() + 1;
            line
        })
        .collect()
}

/// The hash a tag keeps for `line`, read after `parent`.
fn tag(parent: &str, line: &str) -> String {
    let hash = [
        parent.trim_matches(TRIMMED),
        BETWEEN,
        line.trim_matches(TRIMMED),
    ]
    .into_iter()
    .flat_map(str::bytes)
    .fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(FNV_PRIME)
    });
    format!("{:03x}", hash & TAG_BITS)
}

/// `text` with every line tagged, as `file_read_hashed` answers.
fn tagged(text: &str) -> String {
    let mut parent = "";
    let mut out = String::with_capacity(text.len() * 2);
    for (index, line) in lines(text).iter().enumerate() {
        // Writing into a `String` cannot fail.
        let _never_fails = writeln!(
            out,
            "{TAG_START}{}{TAG_SEPARATOR}{}{TAG_END}{}",
            index + 1,
            tag(parent, line.text),
            line.text
        );
        parent = line.text;
    }
    out
}

/// A tag as the model gives it back: the line it was read at, and its hash.
#[derive(Debug, Clone, Copy)]
struct Target<'tag> {
    line: usize,
    hash: &'tag str,
}

/// Where a tag's hash was found near its line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Located {
    At(usize),
    Missing,
    Twice,
}

impl<'tag> Target<'tag> {
    /// `given` as a tag, or the refusal.
    fn parse(given: &'tag str) -> Result<Self, ToolOutput> {
        given
            .strip_prefix(TAG_START)
            .and_then(|rest| rest.split_once(TAG_SEPARATOR))
            .filter(|(_number, hash)| hash.len() == TAG_LEN)
            .and_then(|(number, hash)| number.parse().ok().map(|line| Self { line, hash }))
            .filter(|target| target.line > 0)
            .ok_or_else(|| {
                ToolOutput::failed(
                    ToolErrorCode::InvalidArguments,
                    &format!(
                        "{given:?} is not a tag; one reads L<line>:<hash>, as file_read_hashed \
                         printed it"
                    ),
                )
            })
    }

    /// The index of the line this tag names, looked for within [`RADIUS`] of
    /// `hint`; `which` names the argument in a refusal.
    fn locate(
        self,
        lines: &[Line<'_>],
        hint: usize,
        which: &str,
        path: &str,
    ) -> Result<usize, ToolOutput> {
        if self.line > lines.len() {
            return Err(mismatch(&format!(
                "{which} L{}:{} is past the end of {path}, which has {} lines",
                self.line,
                self.hash,
                lines.len()
            )));
        }
        match self.found_near(lines, hint) {
            Located::At(index) => Ok(index),
            Located::Missing => Err(mismatch(&format!(
                "{which} L{}:{} no longer matches {path} near line {}; the file changed, read \
                 it again",
                self.line,
                self.hash,
                hint + 1
            ))),
            Located::Twice => Err(mismatch(&format!(
                "{which} L{}:{} matches {path} more than once near line {}; read it again",
                self.line,
                self.hash,
                hint + 1
            ))),
        }
    }

    /// Where this tag's hash is within [`RADIUS`] of `hint`.
    fn found_near(self, lines: &[Line<'_>], hint: usize) -> Located {
        let from = hint.saturating_sub(RADIUS);
        let to = lines
            .len()
            .min(hint.saturating_add(RADIUS).saturating_add(1));
        let mut found = Located::Missing;
        for (index, line) in lines.iter().enumerate().take(to).skip(from) {
            let parent = index
                .checked_sub(1)
                .and_then(|before| lines.get(before))
                .map_or("", |line| line.text);
            if tag(parent, line.text) == self.hash {
                found = match found {
                    Located::Missing => Located::At(index),
                    Located::At(_) | Located::Twice => return Located::Twice,
                };
            }
        }
        found
    }
}

#[cfg(test)]
#[path = "hashed/tests.rs"]
mod tests;
