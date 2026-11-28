//! `fs/search` on the sandbox's own disk: ripgrep's walker below the path
//! and its line searcher over each file it keeps, answered once.
//!
//! The walk skips a `.git` directory and what a `.gitignore` or
//! `.git/info/exclude` names, inside a repository or not, and follows no
//! link; dotfiles are searched. The path searched is held inside the
//! workspace through the directory handle, and each file is opened through
//! it, so a link swapped in after the walk is refused like any other. The
//! only bound is the frame: an answer that would not fit one is cut and says
//! so.

use std::io;
use std::path::{Path, PathBuf};

use cap_std::fs::OpenOptions;
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::{Searcher, SearcherBuilder, Sink, SinkContext, SinkMatch};
use ignore::WalkBuilder;
use jsonrpsee_types::{Id, Response, ResponsePayload};

use super::{Parents, Workspace, confined};
use crate::api::{Found, Hit, Pattern, Search};
use crate::error::{self, Result};
use crate::protocol::line;

/// The directory a walk never enters.
const SKIPPED: &str = ".git";
/// The byte a searched line ends at: a pattern never matches across it.
const LINE_END: u8 = b'\n';
/// A glob `globset` cannot compile.
const DETAIL_GLOB: &str = "the glob does not compile";
/// A pattern the regex matcher cannot compile.
const DETAIL_PATTERN: &str = "the pattern does not compile";

impl Workspace {
    /// Answers `search` from the workspace, cut to fit a `frame`-byte line.
    pub(in crate::server) fn search(&self, search: &Search, frame: usize) -> Result<Found> {
        let glob = search.glob.as_deref().map(glob_matcher).transpose()?;
        let matcher = search
            .pattern
            .as_ref()
            .map(|pattern| line_matcher(pattern).map(|matcher| (matcher, pattern)))
            .transpose()?;
        let inside = self.inside(&search.path)?;
        let mut files = self.files(inside, &search.path)?;
        files.retain(|(shown, _path)| glob.as_ref().is_none_or(|glob| glob.is_match(shown)));
        files.sort_unstable();
        let limit = usize::try_from(search.limit).unwrap_or(usize::MAX);
        let found = if let Some((matcher, pattern)) = matcher {
            self.lines(files, &matcher, limit, pattern)
        } else {
            listed(files, limit)
        };
        Ok(fitted(found, frame))
    }

    /// Every regular file at or below `inside`, by the path a hit names it by
    /// and its path inside the workspace: a file named alone keeps the name
    /// it was asked by.
    fn files(&self, inside: &Path, asked: &str) -> Result<Vec<(String, PathBuf)>> {
        let metadata = self.dir.metadata(inside).map_err(confined)?;
        if !metadata.is_dir() {
            let alone = metadata
                .is_file()
                .then(|| (asked.to_owned(), inside.to_owned()));
            return Ok(alone.into_iter().collect());
        }
        let top = self.root.join(inside);
        let walk = WalkBuilder::new(&top)
            .standard_filters(false)
            .hidden(false)
            .parents(true)
            .git_ignore(true)
            .git_exclude(true)
            .require_git(false)
            .follow_links(false)
            .filter_entry(|entry| {
                entry.file_name() != SKIPPED || entry.file_type().is_none_or(|kind| !kind.is_dir())
            })
            .build();
        Ok(walk
            .filter_map(core::result::Result::ok)
            .filter(|entry| entry.metadata().is_ok_and(|metadata| metadata.is_file()))
            .filter_map(|entry| {
                let below = entry.path().strip_prefix(&top).ok()?;
                Some((below.to_str()?.to_owned(), inside.join(below)))
            })
            .collect())
    }

    /// The first matching lines and their context in path order; only matches use the limit.
    fn lines(
        &self,
        files: Vec<(String, PathBuf)>,
        matcher: &RegexMatcher,
        mut remaining: usize,
        pattern: &Pattern,
    ) -> Found {
        let mut found = Found::default();
        for (shown, path) in files {
            let mut read = OpenOptions::new();
            read.read(true);
            let Ok(file) = self.regular_at(&path, &mut read, Parents::Keep) else {
                continue;
            };
            let Some(lines) = matched(&file.into_std(), matcher, remaining, pattern) else {
                continue;
            };
            remaining = lines.remaining;
            found
                .hits
                .extend(lines.hits.into_iter().map(|(number, text)| Hit::Line {
                    path: shown.clone(),
                    number,
                    text,
                }));
            if lines.limited {
                found.limited = true;
                break;
            }
        }
        found
    }
}

/// The first `limit` of `files`, and whether there were more.
fn listed(mut files: Vec<(String, PathBuf)>, limit: usize) -> Found {
    let limited = files.len() > limit;
    files.truncate(limit);
    Found {
        hits: files
            .into_iter()
            .map(|(shown, _path)| Hit::File(shown))
            .collect(),
        limited,
        cut: false,
    }
}

/// `glob` as a matcher of whole relative paths, `*` and `?` stopping at a
/// `/`. Built as a set of one, whose build refuses a glob too large to
/// compile where a lone glob's matcher would panic.
fn glob_matcher(glob: &str) -> Result<GlobSet> {
    let refused = |_failure: globset::Error| error::invalid_params(DETAIL_GLOB);
    let glob = GlobBuilder::new(glob)
        .literal_separator(true)
        .build()
        .map_err(refused)?;
    GlobSetBuilder::new().add(glob).build().map_err(refused)
}

/// `pattern` as a matcher of one line: it never matches the line's end,
/// `^` and `$` hold at the line's edges, and `$` matches before a `\r\n`.
fn line_matcher(pattern: &Pattern) -> Result<RegexMatcher> {
    RegexMatcherBuilder::new()
        .fixed_strings(pattern.literal)
        .case_insensitive(pattern.ignore_case)
        .multi_line(true)
        .crlf(true)
        .line_terminator(Some(LINE_END))
        .build(&pattern.text)
        .map_err(|_failure| error::invalid_params(DETAIL_PATTERN))
}

/// Matching lines and their context from one file. A read error or non-text line skips the file.
fn matched(
    file: &std::fs::File,
    matcher: &RegexMatcher,
    remaining: usize,
    pattern: &Pattern,
) -> Option<Lines> {
    let context = usize::try_from(pattern.context).unwrap_or(usize::MAX);
    let mut lines = Lines {
        remaining,
        context: u64::try_from(context).unwrap_or(u64::MAX),
        line_chars: pattern
            .line_chars
            .map(|n| usize::try_from(n).unwrap_or(usize::MAX)),
        through: 0,
        limited: false,
        hits: Vec::new(),
    };
    SearcherBuilder::new()
        .line_number(true)
        .before_context(context)
        .after_context(context)
        .build()
        .search_file(matcher, file, &mut lines)
        .ok()?;
    Some(lines)
}

/// `through` retains trailing context for the last accepted match even if another
/// match inside the same window has already reached the match limit.
struct Lines {
    remaining: usize,
    context: u64,
    line_chars: Option<usize>,
    through: u64,
    limited: bool,
    hits: Vec<(u64, String)>,
}

impl Lines {
    fn line(&mut self, number: u64, bytes: &[u8]) -> io::Result<bool> {
        let line = std::str::from_utf8(bytes).map_err(io::Error::other)?;
        if self.remaining > 0 || number <= self.through {
            let text = line
                .strip_suffix('\n')
                .map_or(line, |line| line.strip_suffix('\r').unwrap_or(line));
            let end = self.line_chars.and_then(|max| text.char_indices().nth(max));
            let kept = end
                .and_then(|(offset, _)| text.get(..offset))
                .unwrap_or(text);
            self.hits.push((number, kept.to_owned()));
        }
        Ok(!self.limited || number < self.through)
    }
}

impl Sink for Lines {
    type Error = io::Error;

    fn matched(&mut self, _searcher: &Searcher, hit: &SinkMatch<'_>) -> io::Result<bool> {
        let number = hit.line_number().unwrap_or_default();
        if self.remaining == 0 {
            self.limited = true;
        } else {
            self.remaining -= 1;
            self.through = number.saturating_add(self.context);
        }
        self.line(number, hit.bytes())
    }

    fn context(&mut self, _searcher: &Searcher, line: &SinkContext<'_>) -> io::Result<bool> {
        let number = line.line_number().unwrap_or_default();
        if self.remaining == 0 && number > self.through {
            return Ok(!self.limited);
        }
        self.line(number, line.bytes())
    }
}

/// `found`, its hits cut from the end until its answer fits one
/// `frame`-byte line, and marked cut when any were.
///
/// The envelope is measured with the longest identifier a client numbers a
/// call with and both flags `false`, the longer spelling, so the answer fits
/// whatever the call's number and flags.
fn fitted(mut found: Found, frame: usize) -> Found {
    let envelope = line(&Response::new(
        ResponsePayload::success(Found::default()),
        Id::Number(u64::MAX),
    ))
    .len()
    .saturating_sub(1);
    let mut room = frame.saturating_sub(envelope);
    let mut kept = 0;
    for hit in &found.hits {
        let comma = usize::from(kept > 0);
        let size = serde_json::to_vec(hit).map_or(usize::MAX, |bytes| bytes.len());
        match size.checked_add(comma).filter(|needed| *needed <= room) {
            Some(needed) => room -= needed,
            None => break,
        }
        kept += 1;
    }
    found.cut = kept < found.hits.len();
    found.hits.truncate(kept);
    found
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
