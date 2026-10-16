//! Statements stay with their domain owner; values and lifecycle words are bound.

pub(crate) mod access;
pub(crate) mod candidate;
pub(crate) mod draft;
pub(crate) mod message;

pub(crate) const CLOCK: &str = const_format::concatcp!("SELECT ", afd_db::statement::NOW_MS);
