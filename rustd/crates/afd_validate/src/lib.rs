//! What every crate needs to bound untrusted input with garde, written once.
//!
//! `docs/REST_API_DESIGN_GUIDELINES.md` §8 puts a bound on the request type,
//! with garde, and keeps the sentence a caller reads beside it. garde covers
//! lengths and ranges; three things it does not are shared by enough crates to
//! live in one leaf instead of being re-spelled in each:
//!
//! - [`rules`]: the `custom` rules garde lacks — [`finite`] (its float `range`
//!   compares with `<` and `>`, so NaN passes), [`nul_free`], [`ascii_digits`]
//!   and [`charset`].
//! - [`Limit`]: a `?limit` proved inside its route's [`Ceiling`], with the
//!   ceiling passed as garde context, so ten routes stop parsing it ten ways.
//! - [`Sentences`]: a route's table from the path a report names to the fixed
//!   sentence it answers, so garde's own text never reaches a caller; a
//!   [`PathTable`] answers a crate's own error variant the same way.
//!
//! No fallible signature here returns an error of ours: the rules answer
//! [`garde::Result`], `Limit` answers a two-variant [`LimitBreak`] each route
//! maps to its own sentence, and `Sentences` cannot fail.

pub mod rules;

mod limit;
mod sentences;

pub use self::limit::{Ceiling, Limit, LimitBreak};
pub use self::rules::{ascii_digits, charset, finite, nul_free};
pub use self::sentences::{PathTable, Sentences};
