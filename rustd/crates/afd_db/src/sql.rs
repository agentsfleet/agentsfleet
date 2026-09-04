//! Splitting a migration file into the statements Postgres will see.
//!
//! A migration arrives as one string and has to reach Postgres one statement at
//! a time, so `;` has to be found — and `;` is only a terminator when the lexer
//! says it is. Inside `'…'`, `"…"`, `$tag$…$tag$`, `-- …` or `/* … */` it is an
//! ordinary byte. Splitting on every `;` truncates the first function body it
//! meets, and the truncated half applies.
//!
//! # Validation is a constructor, not a method
//!
//! The retired daemon's `sql_splitter.zig` exposed `validate(sql)` beside
//! `next()` and trusts every caller to run it first. Here the scan happens in
//! [`SqlStatements::new`], which returns [`SplitError`] instead of an iterator:
//! past that constructor, "these statements might be a truncated string
//! literal" is not a state a caller can hold. That is the whole reason this is
//! a type and not a function.
//!

mod scanner;

use self::scanner::Scanner;

/// The input ended inside a region that never closed.
///
/// Deliberately not part of [`crate::Error`]'s kind set at the point of
/// detection: a malformed migration is a defect in a committed `.sql` file, not
/// a database failure, and the two must not be reported under one code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SplitError {
    /// Input ended inside a `'…'` string literal.
    #[error("unterminated string literal")]
    UnterminatedString,
    /// Input ended inside a `"…"` quoted identifier.
    #[error("unterminated quoted identifier")]
    UnterminatedQuotedIdentifier,
    /// Input ended inside a `$tag$…$tag$` body.
    #[error("unterminated dollar-quoted body")]
    UnterminatedDollarQuote,
    /// Input ended inside a `/* … */` comment.
    #[error("unterminated block comment")]
    UnterminatedBlockComment,
}

/// The statements of one SQL file, in order, already trimmed and never empty.
///
/// Obtained only from [`SqlStatements::new`], which is what makes a malformed
/// file unrepresentable here.
#[derive(Debug)]
pub struct SqlStatements<'a> {
    scanner: Scanner<'a>,
}

impl<'a> SqlStatements<'a> {
    /// Scans `sql` once for structural defects, then hands back the iterator.
    ///
    /// # Errors
    /// Returns the [`SplitError`] naming the region that never closed.
    pub fn new(sql: &'a str) -> Result<Self, SplitError> {
        let mut probe = Scanner::new(sql);
        while probe.next_statement().is_some() {}
        probe.structural_defect().map_or_else(
            || {
                Ok(Self {
                    scanner: Scanner::new(sql),
                })
            },
            Err,
        )
    }
}

impl<'a> Iterator for SqlStatements<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        self.scanner.next_statement()
    }
}
