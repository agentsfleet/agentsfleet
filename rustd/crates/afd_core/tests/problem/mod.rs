//! Public problem metadata: registry integrity, caller guidance and retry semantics.

mod authoring;
mod registry;
mod statuses;

use afd_core::error_code::{self, REGISTRY};
use afd_core::problem::{DOCS_BASE, Problem, entries};
