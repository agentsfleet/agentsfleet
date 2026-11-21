//! What every crate needs to bound untrusted input with garde, written once.
//!
//! HTTP-GARDE (`.orly/standards/domains/http.md`) puts a bound on the request type,
//! with garde, and keeps the sentence a caller reads beside it. garde covers
//! lengths and ranges; three things it does not are shared by enough crates to
//! live in one leaf instead of being re-spelled in each:
//!
//! - [`rules`]: the `custom` rules garde lacks — [`finite`] (its float `range`
//!   compares with `<` and `>`, so NaN passes), [`nul_free`], [`ascii_digits`]
//!   [`charset`] and [`exactly`].
//! - [`Limit`]: a `?limit` proved inside its route's [`Ceiling`], with the
//!   ceiling passed as garde context, so ten routes stop parsing it ten ways.
//! - [`Sentences`]: a route's table from the path a report names to the fixed
//!   sentence it answers, so garde's own text never reaches a caller; a
//!   [`PathTable`] answers a crate's own error variant the same way.
//! - [`TEXT_MAX_CHARS`]: the one ceiling on text a person or a model hands
//!   the system, and [`TEXT_TOO_LONG`], the sentence that refuses past it.
//! - [`DEFINITION_MAX_BYTES`]: the byte bound on a setup Definition, shared
//!   by the Definition parser and the runner's setup report, and
//!   [`DEFINITION_MAX_JSON_BYTES`], the same Definition as a client may
//!   escape it, which a request body limit reads.
//!
//! No fallible signature here returns an error of ours: the rules answer
//! [`garde::Result`], `Limit` answers a two-variant [`LimitBreak`] each route
//! maps to its own sentence, and `Sentences` cannot fail.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod rules;
pub mod spans;

mod checkpoint_links;
mod definition;
mod limit;
mod sentences;
mod serialized;
mod text;

pub use self::checkpoint_links::checkpoint_links;
pub use self::definition::{
    DEFINITION_MAX_BYTES, DEFINITION_MAX_JSON_BYTES, DESCRIPTION_MAX_CHARS,
};
pub use self::limit::{Ceiling, Limit, LimitBreak};
pub use self::rules::{ascii_digits, charset, exactly, finite, not_relative, nul_free};
pub use self::sentences::{PathTable, Sentences};
pub use self::serialized::{serialized_len, serialized_within};
pub use self::text::{TEXT_MAX_CHARS, TEXT_MAX_JSON_BYTES, TEXT_TOO_LONG};
