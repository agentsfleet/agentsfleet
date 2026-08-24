//! The HTTP shell: routes, admission, and the shape of a refusal.
//!
//! # One table, not four
//!
//! The Zig daemon keeps four total switches over the same `Route` union, in
//! four files — `route_table.zig` (middleware chain), `route_scopes.zig`
//! (required capabilities), `route_admission.zig` (shed class) and
//! `route_template.zig` (span template). Each is individually reasonable and
//! together they are a hazard: adding an endpoint means editing four files,
//! and the compiler only catches three of those omissions, because the
//! admission table gave up its exhaustive match for an `else` arm and rebuilt
//! the check as a runtime test over two hand-maintained name lists.
//!
//! [`route`] folds all four into one [`RouteMeta`] per route. A new variant
//! fails the build until every fact about it is stated, in one place, with no
//! list to keep in step.
#![forbid(unsafe_code)]
#![deny(unused_crate_dependencies)]

// `unused_crate_dependencies` is a crate attribute, so it also grades the lib's
// own test target — and fires there for a dev-dependency only the suites in
// `tests/` import, since those are separate crates it cannot see. Naming it
// here is the lint's own documented remedy, and keeps the deny in force for
// everything else.
#[cfg(test)]
use tokio as _;

pub mod envelope;
pub mod route;

pub use self::envelope::{CONTENT_TYPE_PROBLEM_JSON, ProblemResponse};
pub use self::route::{Guard, Route, RouteClass, RouteMeta, Scopes};
