//! A search of the host's files, run on the blocking pool: ripgrep's walker
//! below the path and its line searcher over each file it keeps.
//!
//! The walk skips a `.git` directory and what a `.gitignore` or
//! `.git/info/exclude` names, inside a repository or not, and follows no
//! link; dotfiles are searched. The host user's global ignore file is not
//! read, so a search answers the same on every host.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use grep_regex::RegexMatcher;
use grep_searcher::{Searcher, SearcherBuilder, Sink, SinkContext, SinkMatch};
use ignore::WalkBuilder;

use super::{LocalEnv, failed};
use crate::tools::env::error::Result;
use crate::tools::env::{
    EnvError, EnvErrorKind, Found, Hit, Pattern, Search, glob_matcher, line_matcher,
};

/// The directory a walk never enters.
const SKIPPED: &str = ".git";

impl LocalEnv {
    /// Answers `search` from the files under the root.
    pub(super) async fn search_files(&self, search: &Search) -> Result<Found> {
        let full = self.resolve(&search.path).await?;
        let search = search.clone();
        tokio::task::spawn_blocking(move || searched(&full, &search))
            .await
            .map_err(|failure| EnvError::new(EnvErrorKind::Unavailable, failure.to_string()))?
    }
}

/// `search` over the directory or file at `full`.
fn searched(full: &Path, search: &Search) -> Result<Found> {
    let refused = |reason: String| EnvError::new(EnvErrorKind::Unavailable, reason);
    let glob = search
        .glob
        .as_deref()
        .map(glob_matcher)
        .transpose()
        .map_err(|failure| refused(failure.to_string()))?;
    let matcher = search
        .pattern
        .as_ref()
        .map(|pattern| line_matcher(pattern).map(|matcher| (matcher, pattern)))
        .transpose()
        .map_err(|failure| refused(failure.to_string()))?;
    let mut files = files(full, &search.path)?;
    files.retain(|(shown, _path)| glob.as_ref().is_none_or(|glob| glob.is_match(shown)));
    files.sort_unstable();
    Ok(if let Some((matcher, pattern)) = matcher {
        lines(files, &matcher, search.limit, pattern)
    } else {
        listed(files, search.limit)
    })
}

/// Every regular file at or below `full`, by the path a hit names it by and
/// its place on the host: a file named alone keeps the name it was asked by.
fn files(full: &Path, asked: &str) -> Result<Vec<(String, PathBuf)>> {
    let metadata = std::fs::metadata(full).map_err(|failure| failed(&failure))?;
    if !metadata.is_dir() {
        let alone = metadata
            .is_file()
            .then(|| (asked.to_owned(), full.to_owned()));
        return Ok(alone.into_iter().collect());
    }
    let walk = WalkBuilder::new(full)
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
            let shown = entry.path().strip_prefix(full).ok()?.to_str()?.to_owned();
            Some((shown, entry.into_path()))
        })
        .collect())
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

/// The first matching lines and their context in path order; only matches use the limit.
fn lines(
    files: Vec<(String, PathBuf)>,
    matcher: &RegexMatcher,
    mut remaining: usize,
    pattern: &Pattern,
) -> Found {
    let mut found = Found::default();
    for (shown, path) in files {
        let Ok(file) = File::open(&path) else {
            continue;
        };
        let Some(lines) = matched(&file, matcher, remaining, pattern) else {
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

/// Matching lines and their context from one file. A read error or non-text line skips the file.
fn matched(
    file: &File,
    matcher: &RegexMatcher,
    remaining: usize,
    pattern: &Pattern,
) -> Option<Lines> {
    let context = pattern.context;
    let mut lines = Lines {
        remaining,
        context: u64::try_from(context).unwrap_or(u64::MAX),
        line_chars: pattern.line_chars,
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
