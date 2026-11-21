//! `grep`: the lines of the working directory's files that hold a pattern.

use regex::RegexBuilder;
use schemars::JsonSchema;
use serde::Deserialize;

use super::{LIMIT_REACHED, answer, glob, joined, start, walk};
use crate::tool::ToolOutput;
use crate::tools::env::{Env, MAX_READ_BYTES};
use crate::tools::files::{Answer, settled};
use crate::tools::{Context, Handler, code};

/// The matches `grep` answers with when the model names no limit.
const GREP_DEFAULT: usize = 100;
/// The most characters of one matching line `grep` shows.
const LINE_MAX: usize = 500;

/// `grep`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Lines {
    /// What to look for: a regular expression, or text when `literal`.
    pattern: String,
    /// The directory or file to search, relative to the working directory:
    /// the working directory when absent.
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

backed! {
    /// Finds the lines of files that hold a pattern.
    Grep over Env
}

impl Handler for Grep<'_> {
    const NAME: &'static str = "grep";
    const DESCRIPTION: &'static str = "Search the files in the working directory for a pattern, a \
        regular expression or, with `literal`, text, and read back each matching line as \
        `path:line: text`. `glob` keeps only files whose path matches it. `.git` is skipped, \
        and a line is cut at 500 characters. At most `limit` lines (100 when absent).";
    type Arguments = Lines;

    async fn run(&self, arguments: Lines, _context: Context<'_>) -> ToolOutput {
        settled(grep(self.backend, &arguments).await)
    }
}

async fn grep(env: &dyn Env, arguments: &Lines) -> Answer {
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
            let mut walked = walk(env, root).await?;
            // A directory lists in the file system's order, so files are
            // searched in path order: the same lines, and the same cut at
            // `limit`, on every host.
            walked.files.sort_unstable();
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
