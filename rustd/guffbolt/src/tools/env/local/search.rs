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
use grep_searcher::{Searcher, SearcherBuilder, Sink, SinkMatch};
use ignore::WalkBuilder;

use super::{LocalEnv, failed};
use crate::tools::env::error::Result;
use crate::tools::env::{EnvError, EnvErrorKind, Found, Hit, Search, glob_matcher, line_matcher};

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
        .map(line_matcher)
        .transpose()
        .map_err(|failure| refused(failure.to_string()))?;
    let mut files = files(full, &search.path)?;
    files.retain(|(shown, _path)| glob.as_ref().is_none_or(|glob| glob.is_match(shown)));
    files.sort_unstable();
    Ok(if let Some(matcher) = matcher {
        lines(files, &matcher, search.limit)
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

/// The lines `matcher` finds in `files`, in order, at most `limit`.
fn lines(files: Vec<(String, PathBuf)>, matcher: &RegexMatcher, limit: usize) -> Found {
    let mut found = Found::default();
    for (shown, path) in files {
        let Ok(file) = File::open(&path) else {
            continue;
        };
        let most = limit.saturating_sub(found.hits.len()).saturating_add(1);
        for (number, text) in matched(&file, matcher, most) {
            if found.hits.len() == limit {
                found.limited = true;
                return found;
            }
            let path = shown.clone();
            found.hits.push(Hit::Line { path, number, text });
        }
    }
    found
}

/// The lines of `file` that `matcher` finds, at most `most`; none when one
/// of them is not text, or the file cannot be read.
fn matched(file: &File, matcher: &RegexMatcher, most: usize) -> Vec<(u64, String)> {
    let mut sink = Lines {
        most,
        found: Vec::new(),
        text: true,
    };
    let searched = SearcherBuilder::new()
        .line_number(true)
        .build()
        .search_file(matcher, file, &mut sink);
    if searched.is_err() || !sink.text {
        return Vec::new();
    }
    sink.found
}

/// What one file's search keeps: its matched lines, up to `most`.
struct Lines {
    most: usize,
    found: Vec<(u64, String)>,
    /// Whether every matched line was text.
    text: bool,
}

impl Sink for Lines {
    type Error = io::Error;

    fn matched(&mut self, _searcher: &Searcher, hit: &SinkMatch<'_>) -> io::Result<bool> {
        let Ok(line) = std::str::from_utf8(hit.bytes()) else {
            self.text = false;
            return Ok(false);
        };
        let line = line
            .strip_suffix('\n')
            .map_or(line, |line| line.strip_suffix('\r').unwrap_or(line));
        let number = hit.line_number().unwrap_or_default();
        self.found.push((number, line.to_owned()));
        Ok(self.found.len() < self.most)
    }
}
