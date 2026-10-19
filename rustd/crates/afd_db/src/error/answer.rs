//! A Postgres error answer built without a server, for tests that need one.

use std::borrow::Cow;
use std::error::Error as StdError;
use std::fmt;

use sqlx::error::{DatabaseError, ErrorKind};

/// A Postgres error answer carrying only its SQLSTATE.
#[derive(Debug)]
struct Answer(&'static str);

impl fmt::Display for Answer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "postgres answered {}", self.0)
    }
}

impl StdError for Answer {}

impl DatabaseError for Answer {
    fn message(&self) -> &str {
        self.0
    }

    fn code(&self) -> Option<Cow<'_, str>> {
        Some(Cow::Borrowed(self.0))
    }

    fn as_error(&self) -> &(dyn StdError + Send + Sync + 'static) {
        self
    }

    fn as_error_mut(&mut self) -> &mut (dyn StdError + Send + Sync + 'static) {
        self
    }

    fn into_error(self: Box<Self>) -> Box<dyn StdError + Send + Sync + 'static> {
        self
    }

    fn kind(&self) -> ErrorKind {
        ErrorKind::Other
    }
}

/// `sqlx`'s error for Postgres answering `code` (a SQLSTATE).
#[must_use]
pub fn database_answer(code: &'static str) -> sqlx::Error {
    sqlx::Error::Database(Box::new(Answer(code)))
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the library"
)]
mod tests {
    use sqlx::error::ErrorKind;

    use super::database_answer;

    /// The SQLSTATE a fixture answer is built from.
    const SERIALIZATION_FAILURE: &str = "40001";

    /// A fixture answer reads back the way a server's would to every caller
    /// that inspects one: its code and message, the kind sqlx assigns it, and
    /// the text an `error = %fault` log field renders.
    #[test]
    fn a_fixture_answer_reads_as_a_server_answer() {
        let mut answer = database_answer(SERIALIZATION_FAILURE)
            .into_database_error()
            .expect("the fixture builds a database answer");

        assert_eq!(answer.code().as_deref(), Some(SERIALIZATION_FAILURE));
        assert_eq!(answer.message(), SERIALIZATION_FAILURE);
        assert!(matches!(answer.kind(), ErrorKind::Other));
        assert_eq!(answer.to_string(), "postgres answered 40001");
        assert_eq!(answer.as_error().to_string(), answer.to_string());
        assert!(answer.as_error_mut().source().is_none());
        assert_eq!(answer.into_error().to_string(), "postgres answered 40001");
    }
}
