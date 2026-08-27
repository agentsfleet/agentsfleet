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
use {
    afd_db as _, afd_redis as _, bytes as _, hyper_util as _, object_store as _, tower as _,
    tracing_subscriber as _,
};

pub mod admission;
pub mod auth;
pub mod client;
pub mod envelope;
pub mod etag;
pub mod handler;
pub mod request_id;
pub mod route;
pub mod router;
pub mod server;
pub mod services;

pub use self::admission::{Admission, DEFAULT_MAX_IN_FLIGHT, admit, is_metered};
pub use self::auth::{Authenticator, Planes, RunnerIdentity};
pub use self::envelope::{CONTENT_TYPE_PROBLEM_JSON, ProblemResponse};
pub use self::request_id::{RequestId, UNKNOWN_REQUEST_ID};
pub use self::route::{Guard, Route, RouteClass, RouteMeta, Scopes};
pub use self::router::{Dependencies, ReadyInputs, Serving, ready_decision};
pub use self::server::{MAX_REQUEST_HEADER_BYTES, http1_builder};
pub use self::services::Services;
