//! One search of the workspace, as `fs/search` carries it: the files below a
//! path, or the lines in them that hold a pattern.

use serde::{Deserialize, Serialize};

/// One search below a path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Search {
    /// The directory to walk, or the one file to search, inside the
    /// workspace.
    pub path: String,
    /// A glob each file's path, relative to `path`, must match whole: `*`
    /// and `?` within one directory, `**` across them. A file named as
    /// `path` itself is matched by that name.
    pub glob: Option<String>,
    /// The lines to find; with none, the search answers the files.
    pub pattern: Option<Pattern>,
    /// The most files or lines to answer with.
    pub limit: u64,
}

/// What a line must hold to be found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pattern {
    /// A regular expression, or text when `literal`.
    pub text: String,
    /// Whether `text` is text rather than a regular expression.
    pub literal: bool,
    /// Whether case is ignored.
    pub ignore_case: bool,
    /// Lines before and after each of the first matching lines to include.
    #[serde(default)]
    pub context: u64,
    /// The most characters to carry per line. None returns the complete text.
    pub line_chars: Option<u64>,
}

/// What a search found, in path order, and a line's file in line order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Found {
    /// The files, or the lines, found.
    pub hits: Vec<Hit>,
    /// Whether more than the search's `limit` were there.
    pub limited: bool,
    /// Whether the answer was cut to fit one frame.
    pub cut: bool,
}

/// One file or one line a search found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
