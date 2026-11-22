//! The startup config: which tools a run offers, by name or by a pattern
//! with one `*`, as `read,grep,mcp__radius__*` names them.

use std::fmt;

use crate::tools::Handler;
use crate::tools::files::{ApplyPatch, FileEdit, FileRead, FileWrite, Find, Grep, Ls};
use crate::tools::process::Shell;

/// The one character a pattern may hold to match a family of names.
const WILDCARD: char = '*';

/// [`ToolsConfig::coding`]'s tools: read, write, edit and patch files, run a
/// command, and look around. Each needs only the environment, so a builder
/// given one builds them all.
const CODING: [&str; 8] = [
    FileRead::NAME,
    FileWrite::NAME,
    FileEdit::NAME,
    ApplyPatch::NAME,
    Shell::NAME,
    Ls::NAME,
    Find::NAME,
    Grep::NAME,
];
/// [`ToolsConfig::read_only`]'s tools: read a file and look around, changing
/// nothing.
const READ_ONLY: [&str; 4] = [FileRead::NAME, Ls::NAME, Find::NAME, Grep::NAME];

/// One entry of a config: a name, or a pattern with one `*`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Pattern {
    /// Matches this name alone.
    Name(String),
    /// Matches every name that starts with `prefix` and ends with `suffix`.
    Family {
        /// What comes before the `*`.
        prefix: String,
        /// What comes after it.
        suffix: String,
    },
}

impl Pattern {
    /// Whether `name` is one this entry names.
    pub(super) fn matches(&self, name: &str) -> bool {
        match self {
            Self::Name(named) => named == name,
            Self::Family { prefix, suffix } => name
                .strip_prefix(prefix.as_str())
                .is_some_and(|rest| rest.ends_with(suffix.as_str())),
        }
    }
}

/// What reading a tools config answers.
pub(super) type Result<T, E = InvalidPattern> = core::result::Result<T, E>;

/// An entry that is neither a name nor a pattern with one `*`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidPattern(String);

impl InvalidPattern {
    /// The entry as the config spelled it.
    #[must_use]
    pub fn entry(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InvalidPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} is neither a tool name nor a pattern with one `*`",
            self.0
        )
    }
}

impl std::error::Error for InvalidPattern {}

/// The tools a run offers, in the order the config names them.
///
/// A name offers the tool of that name, and the builder refuses to start
/// when that tool's backend was not given. A pattern with one `*`, such as
/// `mcp__docs__*`, offers every tool whose name it matches and whose backend
/// was given, so `*` alone offers everything the builder can build. A name no
/// tool has offers nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolsConfig {
    patterns: Vec<Pattern>,
}

impl ToolsConfig {
    /// The config naming `entries`, in order.
    ///
    /// # Errors
    /// An entry is empty or holds more than one `*`.
    pub fn new<I, S>(entries: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let patterns = entries
            .into_iter()
            .map(|entry| parsed(entry.as_ref()))
            .collect::<Result<_>>()?;
        Ok(Self { patterns })
    }

    /// The set a coding run starts with: `file_read`, `file_write`,
    /// `file_edit`, `apply_patch`, `shell`, `ls`, `find` and `grep`. Every
    /// one runs over the environment alone, so a builder given
    /// [`with_env`](super::ToolsBuilder::with_env) builds it whole.
    #[must_use]
    pub fn coding() -> Self {
        Self::named(&CODING)
    }

    /// The set that reads and changes nothing: `file_read`, `ls`, `find` and
    /// `grep`, over the environment alone.
    #[must_use]
    pub fn read_only() -> Self {
        Self::named(&READ_ONLY)
    }

    /// The config naming each of `names`, in order.
    fn named(names: &[&str]) -> Self {
        let patterns = names
            .iter()
            .map(|name| Pattern::Name((*name).to_owned()))
            .collect();
        Self { patterns }
    }

    /// Its entries, in order.
    pub(super) fn patterns(&self) -> &[Pattern] {
        &self.patterns
    }
}

/// `entry` as a pattern, or why it is none.
fn parsed(entry: &str) -> Result<Pattern> {
    let refused = || InvalidPattern(entry.to_owned());
    if entry.is_empty() {
        return Err(refused());
    }
    match entry.split_once(WILDCARD) {
        None => Ok(Pattern::Name(entry.to_owned())),
        Some((_prefix, suffix)) if suffix.contains(WILDCARD) => Err(refused()),
        Some((prefix, suffix)) => Ok(Pattern::Family {
            prefix: prefix.to_owned(),
            suffix: suffix.to_owned(),
        }),
    }
}
