//! The scanner behind [`SqlStatements`](super::SqlStatements).
//!
//! Split from `sql.rs` per RULE FLL. The division is the useful one rather
//! than an arbitrary line count: this file is the Postgres lexer — what a byte
//! means and when a `;` is a boundary — while `sql.rs` is the type that hands
//! the result out and the guarantee it carries.

use super::SplitError;

const LINE_COMMENT: &str = "--";
const BLOCK_COMMENT_OPEN: &str = "/*";
const BLOCK_COMMENT_CLOSE: &str = "*/";

/// Where the scanner currently is.
///
/// The Zig original carries `in_single_quote`, `in_double_quote`,
/// `single_quote_backslash_escapes` and `dollar_tag` as four fields whose
/// legal combinations are a convention. They are mutually exclusive states, so
/// here they are one enum and an illegal combination cannot be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode<'a> {
    Sql,
    /// Inside `'…'`. `E'…'` honours backslash escapes; a plain literal does not
    /// (`standard_conforming_strings`), so `\'` closes one and not the other.
    SingleQuote {
        backslash_escapes: bool,
    },
    DoubleQuote,
    /// Inside `$tag$…$tag$`, holding the opening delimiter verbatim — the
    /// closer must match it exactly, so `;` and other tags stay inert.
    DollarQuote {
        tag: &'a str,
    },
}

#[derive(Debug)]
pub(super) struct Scanner<'a> {
    sql: &'a str,
    pos: usize,
    mode: Mode<'a>,
    /// A `/*` that ran to end of input. The comment is consumed by then, so the
    /// defect has to be remembered rather than re-derived from `mode`.
    unterminated_block_comment: bool,
}

impl<'a> Scanner<'a> {
    pub(super) const fn new(sql: &'a str) -> Self {
        Self {
            sql,
            pos: 0,
            mode: Mode::Sql,
            unterminated_block_comment: false,
        }
    }

    /// Which region, if any, the input ended inside.
    pub(super) const fn structural_defect(&self) -> Option<SplitError> {
        match self.mode {
            Mode::SingleQuote { .. } => Some(SplitError::UnterminatedString),
            Mode::DoubleQuote => Some(SplitError::UnterminatedQuotedIdentifier),
            Mode::DollarQuote { .. } => Some(SplitError::UnterminatedDollarQuote),
            Mode::Sql if self.unterminated_block_comment => {
                Some(SplitError::UnterminatedBlockComment)
            }
            Mode::Sql => None,
        }
    }

    /// The byte at `index`, or `None` past the end.
    fn byte_at(&self, index: usize) -> Option<u8> {
        self.sql.as_bytes().get(index).copied()
    }

    /// Whether `marker` begins at the current position.
    fn matches_here(&self, marker: &str) -> bool {
        self.sql
            .get(self.pos..)
            .is_some_and(|rest| rest.starts_with(marker))
    }

    /// `sql[start..end]`, trimmed. Every boundary this is called with is an
    /// ASCII byte (a `;`, or the first byte after whitespace), so it can never
    /// land inside a multi-byte character; `get` is used anyway because the
    /// workspace denies panicking in library code, and an empty statement — one
    /// that simply never applies — is the right shape for an impossible case.
    fn slice(&self, start: usize, end: usize) -> &'a str {
        self.sql.get(start..end).unwrap_or_default().trim()
    }

    fn skip_line_comment(&mut self) {
        while let Some(byte) = self.byte_at(self.pos) {
            if byte == b'\n' {
                return;
            }
            self.pos += 1;
        }
    }

    /// Postgres block comments nest, so this counts depth rather than scanning
    /// for the first `*/`.
    fn skip_block_comment(&mut self) {
        let mut depth = 0_u32;
        while self.pos < self.sql.len() {
            if self.matches_here(BLOCK_COMMENT_OPEN) {
                depth += 1;
                self.pos += BLOCK_COMMENT_OPEN.len();
            } else if self.matches_here(BLOCK_COMMENT_CLOSE) {
                depth = depth.saturating_sub(1);
                self.pos += BLOCK_COMMENT_CLOSE.len();
                if depth == 0 {
                    return;
                }
            } else {
                self.pos += 1;
            }
        }
        self.unterminated_block_comment = true;
    }

    /// Advances past whitespace and comments so a leading comment is not part
    /// of the statement that follows it.
    fn skip_gap(&mut self) {
        while let Some(byte) = self.byte_at(self.pos) {
            if byte.is_ascii_whitespace() {
                self.pos += 1;
            } else if self.matches_here(LINE_COMMENT) {
                self.skip_line_comment();
            } else if self.matches_here(BLOCK_COMMENT_OPEN) {
                self.skip_block_comment();
            } else {
                return;
            }
        }
    }

    /// Whether the `'` at `pos` opens an `E'…'` escape string — that is, a
    /// standalone `E` precedes it rather than the tail of an identifier such as
    /// `CASE` or `table_e`.
    fn opens_escape_string(&self, pos: usize) -> bool {
        let Some(prev_index) = pos.checked_sub(1) else {
            return false;
        };
        if !matches!(self.byte_at(prev_index), Some(b'E' | b'e')) {
            return false;
        }
        prev_index
            .checked_sub(1)
            .is_none_or(|before| self.byte_at(before).is_none_or(|b| !is_identifier_byte(b)))
    }

    /// Consumes one byte of a `'…'` literal, leaving the mode when it closes.
    fn consume_single_quote(&mut self, byte: u8, backslash_escapes: bool) {
        if backslash_escapes && byte == b'\\' && self.byte_at(self.pos + 1).is_some() {
            self.pos += 2;
            return;
        }
        if byte == b'\'' {
            if self.byte_at(self.pos + 1) == Some(b'\'') {
                self.pos += 2;
                return;
            }
            self.mode = Mode::Sql;
        }
        self.pos += 1;
    }

    /// Consumes one byte of a `"…"` identifier, leaving the mode when it closes.
    fn consume_double_quote(&mut self, byte: u8) {
        if byte == b'"' {
            if self.byte_at(self.pos + 1) == Some(b'"') {
                self.pos += 2;
                return;
            }
            self.mode = Mode::Sql;
        }
        self.pos += 1;
    }

    /// Consumes one byte of a dollar-quoted body, leaving on the matching tag.
    fn consume_dollar_quote(&mut self, byte: u8, tag: &'a str) {
        if byte == b'$' && self.matches_here(tag) {
            self.mode = Mode::Sql;
            self.pos += tag.len();
        } else {
            self.pos += 1;
        }
    }

    /// Handles one byte of ordinary SQL; `Some(end)` means a statement ended.
    fn consume_sql(&mut self, byte: u8) -> Option<usize> {
        if self.matches_here(LINE_COMMENT) {
            self.skip_line_comment();
            return None;
        }
        if self.matches_here(BLOCK_COMMENT_OPEN) {
            self.skip_block_comment();
            return None;
        }
        match byte {
            b'\'' => {
                self.mode = Mode::SingleQuote {
                    backslash_escapes: self.opens_escape_string(self.pos),
                };
                self.pos += 1;
            }
            b'"' => {
                self.mode = Mode::DoubleQuote;
                self.pos += 1;
            }
            b'$' if self.starts_dollar_quote() => {
                if let Some(tag) = self.dollar_delimiter_here() {
                    self.mode = Mode::DollarQuote { tag };
                    self.pos += tag.len();
                } else {
                    self.pos += 1;
                }
            }
            b';' => {
                let end = self.pos;
                self.pos += 1;
                return Some(end);
            }
            _ => self.pos += 1,
        }
        None
    }

    /// A `$` opens a delimiter only when no identifier byte precedes it —
    /// maximal munch folds `a$b$` into one identifier rather than an opener.
    fn starts_dollar_quote(&self) -> bool {
        self.pos
            .checked_sub(1)
            .is_none_or(|prev| self.byte_at(prev).is_none_or(|b| !is_identifier_byte(b)))
    }

    /// The whole delimiter at `pos` (`$$` or `$tag$`), or `None` when this is a
    /// positional parameter such as `$1` or a lone `$`.
    fn dollar_delimiter_here(&self) -> Option<&'a str> {
        let mut index = self.pos + 1;
        while let Some(byte) = self.byte_at(index) {
            if byte == b'$' {
                return self.sql.get(self.pos..=index);
            }
            let valid = if index == self.pos + 1 {
                is_tag_start_byte(byte)
            } else {
                is_tag_byte(byte)
            };
            if !valid {
                return None;
            }
            index += 1;
        }
        None
    }

    /// The next non-empty statement, or `None` when the input is exhausted.
    ///
    /// A loop rather than recursion: `;;;;…` must not grow the stack, because
    /// the validating scan in [`SqlStatements::new`] walks this same path.
    pub(super) fn next_statement(&mut self) -> Option<&'a str> {
        loop {
            self.skip_gap();
            let start = self.pos;
            if start >= self.sql.len() {
                return None;
            }

            let mut terminator = None;
            while let Some(byte) = self.byte_at(self.pos) {
                match self.mode {
                    Mode::SingleQuote { backslash_escapes } => {
                        self.consume_single_quote(byte, backslash_escapes);
                    }
                    Mode::DoubleQuote => self.consume_double_quote(byte),
                    Mode::DollarQuote { tag } => self.consume_dollar_quote(byte, tag),
                    Mode::Sql => {
                        if let Some(end) = self.consume_sql(byte) {
                            terminator = Some(end);
                            break;
                        }
                    }
                }
            }

            let statement = match terminator {
                Some(end) => self.slice(start, end),
                None => self.slice(start, self.sql.len()),
            };
            if !statement.is_empty() {
                return Some(statement);
            }
            // Empty and terminated means `;;` — rescan. Empty and unterminated
            // means the input is spent.
            terminator?;
        }
    }
}

/// Postgres dollar-tag start: `[A-Za-z\200-\377_]`.
const fn is_tag_start_byte(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80
}

/// Postgres dollar-tag continuation: a start byte or a digit.
const fn is_tag_byte(byte: u8) -> bool {
    is_tag_start_byte(byte) || byte.is_ascii_digit()
}

/// Postgres `ident_cont`: `[A-Za-z0-9_$\200-\377]`.
const fn is_identifier_byte(byte: u8) -> bool {
    is_tag_byte(byte) || byte == b'$'
}
