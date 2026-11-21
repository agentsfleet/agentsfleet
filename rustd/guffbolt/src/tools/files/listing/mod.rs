//! `ls`, `find` and `grep`: looking around the working directory without changing
//! it.
//!
//! All three read through the environment's file calls, never a spawned
//! `find` or `rg`, so they work wherever the file tools do and start no
//! process. A walk skips `.git`, follows no link and says when a directory
//! listing stopped early; every answer is cut to a command's output budget.

mod grep;

use std::borrow::Cow;
use std::collections::VecDeque;

use globset::GlobBuilder;
use regex::bytes::Regex;
use schemars::JsonSchema;
use serde::Deserialize;

use super::{Answer, failed, inside, settled};
use crate::tool::ToolOutput;
use crate::tools::env::{EntryKind, Env};
use crate::tools::process::output;
use crate::tools::{Context, Handler, code};

pub use self::grep::{Grep, Lines};

/// The entries `ls` answers with when the model names no limit.
const LS_DEFAULT: usize = 500;
/// The paths `find` answers with when the model names no limit.
const FIND_DEFAULT: usize = 1000;
/// The directory a walk never enters.
const SKIPPED: &str = ".git";
/// The root, as a walk names it.
const HERE: &str = ".";
/// What an answer that stopped at its limit says last.
const LIMIT_REACHED: &str = "... limit reached; narrow the path or the pattern";
/// What a walk whose directory listing stopped early says last.
const WALK_CUT: &str = "... the walk stopped early; narrow the path";

/// `ls`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Directory {
    /// The directory to list, relative to the working directory: the working
    /// directory when absent.
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
    /// The directory to search, relative to the working directory: the
    /// working directory when absent.
    #[serde(default)]
    path: Option<String>,
    /// The most paths to answer with: 1000 when absent.
    #[serde(default)]
    limit: Option<usize>,
}

backed! {
    /// Lists a directory.
    Ls over Env
}

impl Handler for Ls<'_> {
    const NAME: &'static str = "ls";
    const DESCRIPTION: &'static str = "List a directory in the working directory, sorted by \
        name, a directory marked with a trailing slash. Dotfiles are listed. At most `limit` \
        entries (500 when absent).";
    type Arguments = Directory;

    async fn run(&self, arguments: Directory, _context: Context<'_>) -> ToolOutput {
        settled(list(self.backend, &arguments).await)
    }
}

async fn list(env: &dyn Env, arguments: &Directory) -> Answer {
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
        env,
        names,
        cut.then_some(LIMIT_REACHED),
    )))
}

backed! {
    /// Finds files by a glob.
    Find over Env
}

impl Handler for Find<'_> {
    const NAME: &'static str = "find";
    const DESCRIPTION: &'static str = "Find files in the working directory whose path matches \
        a glob, such as `**/*.rs`, and read back their paths relative to the directory searched. \
        `.git` is skipped. At most `limit` paths (1000 when absent).";
    type Arguments = Search;

    async fn run(&self, arguments: Search, _context: Context<'_>) -> ToolOutput {
        settled(find(self.backend, &arguments).await)
    }
}

async fn find(env: &dyn Env, arguments: &Search) -> Answer {
    let root = start(env, arguments.path.as_deref())?;
    let glob = glob(&arguments.pattern)?;
    let limit = arguments.limit.unwrap_or(FIND_DEFAULT);
    let walked = walk(env, root).await?;
    let mut found: Vec<String> = walked
        .files
        .into_iter()
        .filter(|relative| glob.is_match(relative.as_bytes()))
        .collect();
    found.sort_unstable();
    let note = if found.len() > limit {
        Some(LIMIT_REACHED)
    } else {
        walked.cut.then_some(WALK_CUT)
    };
    found.truncate(limit);
    Ok(ToolOutput::succeeded(answer(env, found, note)))
}

/// The path a call starts from: the model's, checked, or the root.
fn start<'p>(env: &dyn Env, path: Option<&'p str>) -> Result<&'p str, ToolOutput> {
    match path {
        Some(path) if !path.is_empty() && path != HERE => inside(env, path),
        _root => Ok(HERE),
    }
}

/// `glob` as a matcher of whole relative paths: `*` and `?` stay within one
/// directory, `**` crosses them. A glob with no directory in it matches a
/// file's name at any depth.
///
/// `globset` translates the glob; the `regex` crate compiles it, under its
/// default size limit, so a glob too large to compile is refused rather than
/// built.
fn glob(glob: &str) -> Result<Regex, ToolOutput> {
    let glob = if glob.contains('/') || glob.starts_with("**") {
        Cow::Borrowed(glob)
    } else {
        Cow::Owned(format!("**/{glob}"))
    };
    let refused = |reason: String| ToolOutput::failed(code::INVALID_ARGUMENTS, &reason);
    let built = GlobBuilder::new(&glob)
        .literal_separator(true)
        .build()
        .map_err(|error| refused(error.to_string()))?;
    Regex::new(built.regex()).map_err(|error| refused(error.to_string()))
}

/// What a walk found: every file below its root, by its path relative to
/// that root, and whether it stopped early.
struct Walked {
    files: Vec<String>,
    cut: bool,
}

/// Every file below `root`, breadth first, skipping [`SKIPPED`] and every
/// link.
async fn walk(env: &dyn Env, root: &str) -> Result<Walked, ToolOutput> {
    let mut walked = Walked {
        files: Vec::new(),
        cut: false,
    };
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

/// `lines`, one to a line, cut to a command's output budget in `env`, with
/// `note` last when there is one.
fn answer(env: &dyn Env, lines: Vec<String>, note: Option<&str>) -> String {
    let budget = output::budget(None, env.edge_bytes());
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
