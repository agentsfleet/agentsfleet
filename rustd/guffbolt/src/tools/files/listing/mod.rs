//! `ls`, `find` and `grep`: looking around the working directory without changing
//! it.
//!
//! `ls` lists one directory through the environment's file calls. `find` and
//! `grep` ask the environment for one search, which walks where the files
//! are, skips `.git` and what `.gitignore` names, and follows no link; no
//! `find` or `rg` process is started. Every answer is cut to a command's
//! output budget, and says when the environment cut it first.

mod grep;

use std::borrow::Cow;

use schemars::JsonSchema;
use serde::Deserialize;

use super::{Answer, failed, inside, settled};
use crate::tool::ToolOutput;
use crate::tools::env::{EntryKind, Env, Found, Hit, Search as Query, glob_matcher};
use crate::tools::process::output;
use crate::tools::{Context, Handler, code};

pub use self::grep::{Grep, Lines};

/// The entries `ls` answers with when the model names no limit.
const LS_DEFAULT: usize = 500;
/// The paths `find` answers with when the model names no limit.
const FIND_DEFAULT: usize = 1000;
/// The root, as a call names it.
const HERE: &str = ".";
/// What an answer that stopped at its limit says last.
const LIMIT_REACHED: &str = "... limit reached; narrow the path or the pattern";
/// What a search the environment cut short says last.
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
        `.git` and what `.gitignore` names are skipped. At most `limit` paths (1000 when absent).";
    type Arguments = Search;

    async fn run(&self, arguments: Search, _context: Context<'_>) -> ToolOutput {
        settled(find(self.backend, &arguments).await)
    }
}

async fn find(env: &dyn Env, arguments: &Search) -> Answer {
    let search = Query {
        path: start(env, arguments.path.as_deref())?.to_owned(),
        glob: Some(glob(&arguments.pattern)?),
        pattern: None,
        limit: arguments.limit.unwrap_or(FIND_DEFAULT),
    };
    let found = env
        .search(&search)
        .await
        .map_err(|failure| failed(&failure))?;
    let note = if found.limited {
        Some(LIMIT_REACHED)
    } else {
        found.cut.then_some(WALK_CUT)
    };
    Ok(ToolOutput::succeeded(answer(env, shown(found), note)))
}

/// The path a call starts from: the model's, checked, or the root.
fn start<'p>(env: &dyn Env, path: Option<&'p str>) -> Result<&'p str, ToolOutput> {
    match path {
        Some(path) if !path.is_empty() && path != HERE => inside(env, path),
        _root => Ok(HERE),
    }
}

/// `glob` as a search matches it, against whole relative paths: a glob with
/// no directory in it matches a file's name at any depth. One `globset`
/// cannot compile is refused rather than sent.
fn glob(glob: &str) -> Result<String, ToolOutput> {
    let glob = if glob.contains('/') || glob.starts_with("**") {
        Cow::Borrowed(glob)
    } else {
        Cow::Owned(format!("**/{glob}"))
    };
    glob_matcher(&glob)
        .map_err(|refused| ToolOutput::failed(code::INVALID_ARGUMENTS, &refused.to_string()))?;
    Ok(glob.into_owned())
}

/// What a search found, one line each: a file by its path, a line as
/// `path:line: text`.
fn shown(found: Found) -> Vec<String> {
    found
        .hits
        .into_iter()
        .map(|hit| match hit {
            Hit::File(path) => path,
            Hit::Line { path, number, text } => format!("{path}:{number}: {text}"),
        })
        .collect()
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
