//! One search of an environment's files: the files below a path, or the
//! lines in them that hold a pattern, asked once and answered once by the
//! environment that holds the files.

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use grep_regex::{RegexMatcher, RegexMatcherBuilder};

/// The byte a searched line ends at: a pattern never matches across it.
const LINE_END: u8 = b'\n';

/// One search below a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Search {
    /// The directory to walk, or the one file to search, relative to the
    /// root.
    pub path: String,
    /// A glob each file's path, relative to `path`, must match whole: `*`
    /// and `?` within one directory, `**` across them. A file named as
    /// `path` itself is matched by that name.
    pub glob: Option<String>,
    /// The lines to find; with none, the search answers the files.
    pub pattern: Option<Pattern>,
    /// The most files or lines to answer with.
    pub limit: usize,
}

/// What a line must hold to be found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    /// A regular expression, or text when `literal`.
    pub text: String,
    /// Whether `text` is text rather than a regular expression.
    pub literal: bool,
    /// Whether case is ignored.
    pub ignore_case: bool,
    /// Lines before and after each of the first matching lines to include.
    pub context: usize,
    /// The most characters to carry per line. None returns the complete text.
    pub line_chars: Option<usize>,
}

/// What a search found, in path order, and a line's file in line order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Found {
    /// The files, or the lines, found.
    pub hits: Vec<Hit>,
    /// Whether more than the search's `limit` were there.
    pub limited: bool,
    /// Whether the environment cut the answer short to carry it, such as to
    /// fit one message of its transport.
    pub cut: bool,
}

/// One file or one line a search found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hit {
    /// A file, by its path relative to the path searched.
    File(String),
    /// A matching line or a line within its requested context.
    Line {
        /// Its file, by its path relative to the path searched.
        path: String,
        /// Its number, counting from 1.
        number: u64,
        /// The line, without its ending.
        text: String,
    },
}

/// `glob` as a matcher of whole relative paths, `*` and `?` stopping at a
/// `/`. Built as a set of one, whose build refuses a glob too large to
/// compile where a lone glob's matcher would panic.
pub(crate) fn glob_matcher(glob: &str) -> Result<GlobSet, globset::Error> {
    let glob = GlobBuilder::new(glob).literal_separator(true).build()?;
    GlobSetBuilder::new().add(glob).build()
}

/// `pattern` as a matcher of one line: it never matches the line's end,
/// `^` and `$` hold at the line's edges, and `$` matches before a `\r\n`.
pub(crate) fn line_matcher(pattern: &Pattern) -> Result<RegexMatcher, grep_regex::Error> {
    RegexMatcherBuilder::new()
        .fixed_strings(pattern.literal)
        .case_insensitive(pattern.ignore_case)
        .multi_line(true)
        .crlf(true)
        .line_terminator(Some(LINE_END))
        .build(&pattern.text)
}
