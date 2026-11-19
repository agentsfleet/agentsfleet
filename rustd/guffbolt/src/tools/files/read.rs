//! `file_read`: a window of a file's lines, cut to the model's output budget.
//!
//! A read starts at line `offset` (the first when absent) and carries at most
//! `limit` lines (2,000 when absent, the default Codex's `read_file` had),
//! never more bytes than a command's output reads back. A read that stops
//! before the file ends says on its last line where the next one starts, so a
//! model pages a large file instead of taking it whole into its window. The
//! environment reads at most `MAX_READ_BYTES` of a file, from its top; a file
//! longer than that is paged up to there, and its last line names `shell` for
//! the rest.

use std::num::NonZeroUsize;

use schemars::JsonSchema;
use serde::Deserialize;

use super::{Answer, failed, inside, settled};
use crate::tool::ToolOutput;
use crate::tools::env::MAX_READ_BYTES;
use crate::tools::process::{env_of, output};
use crate::tools::{Context, Handler, code};

/// The lines a read carries when the model names no limit.
pub const LINES_DEFAULT: usize = 2000;
/// How a read that stopped before the file ended says where to go on.
const CONTINUES_AT: &str = "... the file continues at line";
const READ_ON: &str = "read on with offset";
/// How a line longer than the whole budget is marked where it was cut.
const CUT_AT: &str = "was cut at";
/// How a read that reached the end of what one environment read carries ends.
const CONTINUES_PAST: &str = "... the file continues past";
const READ_REST: &str = "read the rest with shell";

/// `file_read`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Window {
    /// The file, relative to the workspace.
    path: String,
    /// The line to start at, counting from 1: the first when absent.
    #[serde(default)]
    offset: Option<NonZeroUsize>,
    /// The most lines to read: 2000 when absent.
    #[serde(default)]
    limit: Option<NonZeroUsize>,
}

/// Reads part of a file.
#[derive(Debug)]
pub struct FileRead;

impl Handler for FileRead {
    const NAME: &'static str = "file_read";
    const DESCRIPTION: &'static str = "Read a file in the workspace and read back its text: \
        at most `limit` lines (2000 when absent) from line `offset` (1 when absent), cut to \
        the same output budget as a command's. A read that stops before the file ends says on \
        its last line which offset to read on from.";
    type Arguments = Window;

    async fn run(&self, arguments: Window, context: Context<'_>) -> ToolOutput {
        settled(read(&context, &arguments).await)
    }
}

async fn read(context: &Context<'_>, window: &Window) -> Answer {
    let env = env_of(context)?;
    let path = inside(env, &window.path)?;
    let fetched = env
        .read_file(path, MAX_READ_BYTES)
        .await
        .map_err(|failure| failed(&failure))?;
    let text = String::from_utf8_lossy(&fetched.data);
    let first = window.offset.map_or(1, NonZeroUsize::get);
    let limit = window.limit.map_or(LINES_DEFAULT, NonZeroUsize::get);
    let page = Page::of(&text, first, limit, output::budget(None)).ok_or_else(|| {
        let lines = text.split_inclusive('\n').count();
        let past = if fetched.truncated {
            format!("the {lines} lines of the first {MAX_READ_BYTES} bytes of {path}; {READ_REST}")
        } else {
            format!("the end of {path}, which has {lines} lines")
        };
        ToolOutput::failed(
            code::INVALID_ARGUMENTS,
            &format!("offset {first} is past {past}"),
        )
    })?;
    Ok(ToolOutput::succeeded(page.read_back(fetched.truncated)))
}

/// The lines one read carries, and where the next read would start.
#[derive(Debug, PartialEq, Eq)]
pub struct Page {
    /// The lines read.
    pub text: String,
    /// The line the next read starts at; none when this one reached the end.
    pub next: Option<usize>,
    /// A line alone past the budget, cut short: its number, the bytes kept
    /// and the bytes it has.
    pub cut: Option<(usize, usize, usize)>,
}

impl Page {
    /// Lines `first..` of `text`, at most `limit` of them and `budget` bytes;
    /// `None` when `first` is past the last line. A first line longer than the
    /// whole budget is cut on a character boundary, so every read moves on.
    #[must_use]
    pub fn of(text: &str, first: usize, limit: usize, budget: usize) -> Option<Self> {
        let mut lines = text.split_inclusive('\n').skip(first - 1).peekable();
        if lines.peek().is_none() && first > 1 {
            return None;
        }
        let mut page = String::new();
        let mut taken = 0;
        let mut cut = None;
        // A cut line ends its page: the cut may have moved back to a
        // character boundary, and the room that leaves is not for the next
        // line, which would read as the cut line's end.
        while let Some(line) = lines.next_if(|line| {
            taken < limit && cut.is_none() && (page.len() + line.len() <= budget || taken == 0)
        }) {
            if line.len() > budget {
                let kept = line.floor_char_boundary(budget);
                page.push_str(line.get(..kept).unwrap_or_default());
                cut = Some((first, kept, line.len()));
            } else {
                page.push_str(line);
            }
            taken += 1;
        }
        let next = lines.peek().map(|_more| first + taken);
        Some(Self {
            text: page,
            next,
            cut,
        })
    }

    /// What the model reads: the lines, then what it is owed about the rest.
    #[must_use]
    pub fn read_back(self, beyond_one_read: bool) -> String {
        let mut said = self.text;
        if let Some((number, kept, bytes)) = self.cut {
            let note = format!("... line {number} {CUT_AT} {kept} of its {bytes} bytes");
            said = output::with_line(said, &note);
        }
        match self.next {
            Some(next) => {
                output::with_line(said, &format!("{CONTINUES_AT} {next}; {READ_ON} {next}"))
            }
            None if beyond_one_read => output::with_line(
                said,
                &format!("{CONTINUES_PAST} {MAX_READ_BYTES} bytes; {READ_REST}"),
            ),
            None => said,
        }
    }
}
