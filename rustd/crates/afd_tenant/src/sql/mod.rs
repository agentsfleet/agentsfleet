//! Every statement this crate runs, collected, and nothing else.
//!
//! Split by DOMAIN rather than by line count, for the reason `afd_fleet::sql`
//! records: Rust has real modules, so no file needs a re-export to stay
//! findable and `grep -rn 'SELECT' src/sql/` still returns everything.
//!
//! The statements are byte-identical to their Zig originals. Row-equivalence is
//! the cutover invariant, so a statement is copied rather than re-derived;
//! where a `$n` order looks odd, it is odd in the original too.

pub mod apikey;
pub mod billing;
pub mod cli_credential;
pub mod workspace;
