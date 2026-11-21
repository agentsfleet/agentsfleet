//! `grep`: the lines of the working directory's files that hold a pattern.

use schemars::JsonSchema;
use serde::Deserialize;

use super::{LIMIT_REACHED, answer, glob, shown, start};
use crate::tool::ToolOutput;
use crate::tools::env::{Env, EnvErrorKind, Pattern, Search, line_matcher};
use crate::tools::files::{Answer, failed, settled};
use crate::tools::{Context, Handler, code};

/// The matches `grep` answers with when the model names no limit.
const GREP_DEFAULT: usize = 100;

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
        `path:line: text`. `glob` keeps only files whose path matches it. `.git` and what \
        `.gitignore` names are skipped. At most `limit` lines (100 when absent).";
    type Arguments = Lines;

    async fn run(&self, arguments: Lines, _context: Context<'_>) -> ToolOutput {
        settled(grep(self.backend, &arguments).await)
    }
}

async fn grep(env: &dyn Env, arguments: &Lines) -> Answer {
    let pattern = Pattern {
        text: arguments.pattern.clone(),
        literal: arguments.literal,
        ignore_case: arguments.ignore_case,
    };
    line_matcher(&pattern)
        .map_err(|refused| ToolOutput::failed(code::INVALID_ARGUMENTS, &refused.to_string()))?;
    let search = Search {
        path: start(env, arguments.path.as_deref())?.to_owned(),
        glob: arguments.glob.as_deref().map(glob).transpose()?,
        pattern: Some(pattern),
        limit: arguments.limit.unwrap_or(GREP_DEFAULT),
    };
    let found = match env.search(&search).await {
        Ok(found) => found,
        // A path that is not there holds no line.
        Err(failure) if failure.kind() == EnvErrorKind::NotFound => {
            return Ok(ToolOutput::succeeded(String::new()));
        }
        Err(failure) => return Err(failed(&failure)),
    };
    let note = (found.limited || found.cut).then_some(LIMIT_REACHED);
    Ok(ToolOutput::succeeded(answer(env, shown(found), note)))
}
