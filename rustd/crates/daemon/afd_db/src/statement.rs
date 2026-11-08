//! What every crate building a Postgres statement shares: the bound-query type
//! and the escape that keeps a caller's text literal inside a `LIKE` pattern.
//!
//! The clock is not here. Each `.sql` file names the clock function its job
//! needs (SQL-STAMP and SQL-DEADLINE, `.orly/standards/domains/sql.md`), so a reader sees in
//! the statement itself whether it reads the time before or after a lock.

use sqlx::postgres::PgArguments;
use sqlx::query::Query;

/// A statement bound and ready to execute.
///
/// Named once so a binder's signature reads as one line rather than three of
/// sqlx generics, and so a crate that only builds statements does not link a
/// sibling crate to borrow the spelling.
pub type Bound<'q> = Query<'q, sqlx::Postgres, PgArguments>;

/// The escape character a `LIKE` pattern built by [`like_escape`] relies on,
/// which is Postgres's default and what a statement's `ESCAPE` clause names.
pub const LIKE_ESCAPE: char = '\\';

/// The `LIKE` wildcard matching any run of characters, which a caller places
/// around or between escaped pieces.
pub const LIKE_ANY: char = '%';

/// The characters a `LIKE` pattern reads as other than themselves.
const LIKE_METACHARACTERS: [char; 3] = [LIKE_ANY, '_', LIKE_ESCAPE];

/// `text` with every `LIKE` metacharacter escaped, so each matches only itself.
///
/// The escape character is escaped too. Its absence is the one Postgres treats
/// as an ERROR rather than a wrong match: a pattern ending in a lone `\` is an
/// unterminated escape sequence (SQLSTATE 22025), which reaches the caller as a
/// 500 on a filter they are entitled to type. A caller adds its own wildcards
/// around or between escaped pieces.
#[must_use]
pub fn like_escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if LIKE_METACHARACTERS.contains(&character) {
            escaped.push(LIKE_ESCAPE);
        }
        escaped.push(character);
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::like_escape;

    #[test]
    fn every_metacharacter_is_escaped_and_nothing_else_is() {
        // pin test: literal is the contract
        assert_eq!(like_escape(r"50%_off\*"), r"50\%\_off\\*");
        assert_eq!(like_escape("plain text"), "plain text");
        assert_eq!(like_escape(""), "");
    }

    #[test]
    fn a_trailing_escape_character_is_itself_escaped() {
        // pin test: literal is the contract
        assert_eq!(like_escape(r"dir\"), r"dir\\");
    }
}
