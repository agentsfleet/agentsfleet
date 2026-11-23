//! Public problem metadata: registry integrity, caller guidance and retry semantics.

mod registry;
mod setup;
mod statuses;

use af_core::error_code::{self, REGISTRY};
use af_core::problem::{DOCS_BASE, Problem, entries};
