//! `ls`, `find` and `grep`: looking around the workspace without changing
//! it, in the shape of pi's tools (`coding-agent/src/core/tools/`).
//!
//! All three read through the environment's file calls, never a spawned
//! `find` or `rg`, so they work wherever the file tools do and start no
//! process. A walk skips `.git`, follows no link, visits at most
//! [`WALK_MAX`] entries and says when it stopped early; every answer is cut
//! to a command's output budget.

use std::collections::VecDeque;

use regex::{Regex, RegexBuilder};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{Answer, failed, inside, settled};
use crate::tool::ToolOutput;
use crate::tools::env::{EntryKind, Env, MAX_READ_BYTES};
use crate::tools::process::{env_of, output};
use crate::tools::{Context, Handler, code};

/// The entries `ls` answers with when the model names no limit: pi's.
const LS_DEFAULT: usize = 500;
/// The paths `find` answers with when the model names no limit: pi's.
const FIND_DEFAULT: usize = 1000;
/// The matches `grep` answers with when the model names no limit: pi's.
const GREP_DEFAULT: usize = 100;
/// The most entries one walk visits, so a call over a vast tree ends.
pub const WALK_MAX: usize = 20_000;
/// The most characters of one matching line `grep` shows: pi's.
const LINE_MAX: usize = 500;
/// The directory a walk never enters.
const SKIPPED: &str = ".git";
/// The root, as a walk names it.
const HERE: &str = ".";
/// What an answer that stopped at its limit says last.
const LIMIT_REACHED: &str = "... limit reached; narrow the path or the pattern";
/// What a walk that stopped at its ceiling says last.
const WALK_CUT: &str = "... the walk stopped early; narrow the path";

/// `ls`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Directory {
    /// The directory to list, relative to the workspace: the workspace when
    /// absent.
    #[serde(default)]
    path: Option<String>,
    /// The most entries to list: 500 when absent.
    #[serde(default)]
    limit: Option<usize>,
}

/// `find`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Search {
    /// A glob the path must match, relative to `path`: `*` within one
    /// directory, `**` across them, `?` one character; such as `**/*.rs`.
    pattern: String,
    /// The directory to search, relative to the workspace: the workspace when
    /// absent.
    #[serde(default)]
    path: Option<String>,
    /// The most paths to answer with: 1000 when absent.
    #[serde(default)]
    limit: Option<usize>,
}

/// `grep`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Lines {
    /// What to look for: a regular expression, or text when `literal`.
    pattern: String,
    /// The directory or file to search, relative to the workspace: the
    /// workspace when absent.
    #[serde(default)]
    path: Option<String>,
    /// Only files whose path matches this glob, such as `*.ts`.
    #[serde(default)]
    glob: Option<String>,
    /// Whether case is ignored: not when absent.
    #[serde(default)]
    ignore_case: bool,
    /// Whether `pattern` is text rather than a regular expression.
    #[serde(default)]
    literal: bool,
    /// The most matching lines to answer with: 100 when absent.
    #[serde(default)]
    limit: Option<usize>,
}

/// Lists a directory.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ls;

impl Handler for Ls {
    const NAME: &'static str = "ls";
    const DESCRIPTION: &'static str = "List a directory in the workspace, sorted by name, a \
        directory marked with a trailing slash. Dotfiles are listed. At most `limit` entries \
        (500 when absent).";
    type Arguments = Directory;

    async fn run(&self, arguments: Directory, context: Context<'_>) -> ToolOutput {
        settled(list(&context, &arguments).await)
    }
}

async fn list(context: &Context<'_>, arguments: &Directory) -> Answer {
    let env = env_of(context)?;
    let path = start(env, arguments.path.as_deref())?;
    let listing = env
        .list_dir(path)
        .await
        .map_err(|failure| failed(&failure))?;
    let mut names: Vec<String> = listing
        .entries
        .iter()
        .map(|entry| match entry.kind {
            EntryKind::Directory => format!("{}/", entry.name),
            EntryKind::File | EntryKind::Symlink | EntryKind::Other => entry.name.clone(),
        })
        .collect();
    names.sort_unstable();
    let limit = arguments.limit.unwrap_or(LS_DEFAULT);
    let cut = names.len() > limit || listing.truncated;
    names.truncate(limit);
    Ok(ToolOutput::succeeded(answer(
        names,
        cut.then_some(LIMIT_REACHED),
    )))
}

/// Finds files by a glob.
#[derive(Debug, Clone, Copy, Default)]
pub struct Find;

impl Handler for Find {
    const NAME: &'static str = "find";
    const DESCRIPTION: &'static str = "Find files in the workspace whose path matches a glob, \
        such as `**/*.rs`, and read back their paths relative to the directory searched. \
        `.git` is skipped. At most `limit` paths (1000 when absent).";
    type Arguments = Search;

    async fn run(&self, arguments: Search, context: Context<'_>) -> ToolOutput {
        settled(find(&context, &arguments).await)
    }
}

async fn find(context: &Context<'_>, arguments: &Search) -> Answer {
    let env = env_of(context)?;
    let root = start(env, arguments.path.as_deref())?;
    let glob = glob(&arguments.pattern)?;
    let limit = arguments.limit.unwrap_or(FIND_DEFAULT);
    let walked = walk(env, root).await?;
    let mut found: Vec<String> = walked
        .files
        .into_iter()
        .filter(|relative| glob.is_match(relative))
        .collect();
    found.sort_unstable();
    let note = if found.len() > limit {
        Some(LIMIT_REACHED)
    } else {
        walked.cut.then_some(WALK_CUT)
    };
    found.truncate(limit);
    Ok(ToolOutput::succeeded(answer(found, note)))
}

/// Finds the lines of files that hold a pattern.
#[derive(Debug, Clone, Copy, Default)]
pub struct Grep;

impl Handler for Grep {
    const NAME: &'static str = "grep";
    const DESCRIPTION: &'static str = "Search the files in the workspace for a pattern, a \
        regular expression or, with `literal`, text, and read back each matching line as \
        `path:line: text`. `glob` keeps only files whose path matches it. `.git` is skipped, \
        and a line is cut at 500 characters. At most `limit` lines (100 when absent).";
    type Arguments = Lines;

    async fn run(&self, arguments: Lines, context: Context<'_>) -> ToolOutput {
        settled(grep(&context, &arguments).await)
    }
}

async fn grep(context: &Context<'_>, arguments: &Lines) -> Answer {
    let env = env_of(context)?;
    let root = start(env, arguments.path.as_deref())?;
    let pattern = if arguments.literal {
        regex::escape(&arguments.pattern)
    } else {
        arguments.pattern.clone()
    };
    let matcher = RegexBuilder::new(&pattern)
        .case_insensitive(arguments.ignore_case)
        .build()
        .map_err(|refused| ToolOutput::failed(code::INVALID_ARGUMENTS, &refused.to_string()))?;
    let only = arguments.glob.as_deref().map(glob).transpose()?;
    let limit = arguments.limit.unwrap_or(GREP_DEFAULT);
    // A path that names a file is searched alone; a directory is walked.
    let (files, mut cut) = match env.list_dir(root).await {
        Ok(_listing) => {
            let walked = walk(env, root).await?;
            let files = walked
                .files
                .into_iter()
                .map(|relative| (joined(root, &relative), relative))
                .collect();
            (files, walked.cut)
        }
        Err(_not_a_directory) => (vec![(root.to_owned(), root.to_owned())], false),
    };
    let mut lines = Vec::new();
    'files: for (path, relative) in files {
        if only.as_ref().is_some_and(|glob| !glob.is_match(&relative)) {
            continue;
        }
        let Ok(file) = env.read_file(&path, MAX_READ_BYTES).await else {
            continue;
        };
        // A file that is not text holds no line to show.
        let Ok(text) = std::str::from_utf8(&file.data) else {
            continue;
        };
        for (number, line) in text.lines().enumerate() {
            if !matcher.is_match(line) {
                continue;
            }
            if lines.len() == limit {
                cut = true;
                break 'files;
            }
            let shown: String = line.chars().take(LINE_MAX).collect();
            lines.push(format!("{relative}:{}: {shown}", number + 1));
        }
    }
    let note = cut.then_some(LIMIT_REACHED);
    Ok(ToolOutput::succeeded(answer(lines, note)))
}

/// The path a call starts from: the model's, checked, or the root.
fn start<'p>(env: &dyn Env, path: Option<&'p str>) -> Result<&'p str, ToolOutput> {
    match path {
        Some(path) if !path.is_empty() && path != HERE => inside(env, path),
        _root => Ok(HERE),
    }
}

/// `glob` as the regular expression it means, matched against a whole path.
fn glob(glob: &str) -> Result<Regex, ToolOutput> {
    let mut pattern = String::from("^");
    let mut rest = glob.chars().peekable();
    while let Some(next) = rest.next() {
        match next {
            '*' if rest.peek() == Some(&'*') => {
                rest.next();
                // `**/` also matches no directory at all.
                if rest.peek() == Some(&'/') {
                    rest.next();
                    pattern.push_str("(?:.*/)?");
                } else {
                    pattern.push_str(".*");
                }
            }
            '*' => pattern.push_str("[^/]*"),
            '?' => pattern.push_str("[^/]"),
            other => pattern.push_str(&regex::escape(&other.to_string())),
        }
    }
    pattern.push('$');
    // A glob with no directory in it matches a file's name at any depth.
    if !glob.contains('/') {
        pattern.replace_range(..1, "^(?:.*/)?");
    }
    Regex::new(&pattern)
        .map_err(|refused| ToolOutput::failed(code::INVALID_ARGUMENTS, &refused.to_string()))
}

/// What a walk found: every file below its root, by its path relative to
/// that root, and whether it stopped early.
struct Walked {
    files: Vec<String>,
    cut: bool,
}

/// Every file below `root`, breadth first, skipping [`SKIPPED`] and every
/// link, at most [`WALK_MAX`] entries in all.
async fn walk(env: &dyn Env, root: &str) -> Result<Walked, ToolOutput> {
    let mut walked = Walked {
        files: Vec::new(),
        cut: false,
    };
    let mut visited = 0;
    let mut pending = VecDeque::from([String::new()]);
    while let Some(relative) = pending.pop_front() {
        let directory = if relative.is_empty() {
            root.to_owned()
        } else {
            joined(root, &relative)
        };
        let listing = env
            .list_dir(&directory)
            .await
            .map_err(|failure| failed(&failure))?;
        walked.cut |= listing.truncated;
        for entry in listing.entries {
            visited += 1;
            if visited > WALK_MAX {
                walked.cut = true;
                return Ok(walked);
            }
            let skipped = entry.name == SKIPPED;
            let path = if relative.is_empty() {
                entry.name
            } else {
                format!("{relative}/{}", entry.name)
            };
            match entry.kind {
                EntryKind::Directory if !skipped => pending.push_back(path),
                EntryKind::File => walked.files.push(path),
                EntryKind::Directory | EntryKind::Symlink | EntryKind::Other => {}
            }
        }
    }
    Ok(walked)
}

/// `relative` under `root`, as a file call names it.
fn joined(root: &str, relative: &str) -> String {
    if root == HERE {
        relative.to_owned()
    } else {
        format!("{root}/{relative}")
    }
}

/// `lines`, one to a line, cut to a command's output budget, with `note`
/// last when there is one.
fn answer(lines: Vec<String>, note: Option<&str>) -> String {
    let budget = output::budget(None);
    let mut said = String::new();
    for line in lines {
        if said.len() + line.len() + 1 > budget {
            return output::with_line(said, LIMIT_REACHED);
        }
        said = output::with_line(said, &line);
    }
    match note {
        Some(note) => output::with_line(said, note),
        None => said,
    }
}
