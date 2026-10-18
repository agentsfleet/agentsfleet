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
