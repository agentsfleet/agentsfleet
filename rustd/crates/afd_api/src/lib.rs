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
    afd_admin as _, afd_approval as _, afd_billing as _, afd_connector as _, afd_credential as _,
    afd_cron as _, afd_crypto as _, afd_db as _, afd_events as _, afd_fleet as _,
    afd_fleet_lifecycle as _, afd_fleet_ops as _, afd_identity as _, afd_ingress as _,
    afd_library as _, afd_redis as _, afd_runner as _, afd_sse as _, afd_state as _,
    afd_tenant as _, afd_vault as _, afd_webhook as _, base64 as _, bytes as _, futures_util as _,
    hyper as _, hyper_util as _, jsonwebtoken as _, object_store as _, reqwest as _, serde as _,
    serde_json as _, sha2 as _, sqlx as _, tokio as _, tower as _, tracing_subscriber as _,
};

pub use afd_http::admission;
pub use afd_http::auth;
pub use afd_http::client;
pub use afd_http::envelope;
pub use afd_http::etag;
pub mod handler;
// The document generator, compiled only when it is asked for.
#[cfg(feature = "openapi")]
pub mod openapi;
pub use afd_http::request_id;
pub use afd_http::route;
pub mod router;
pub mod server;
pub use afd_http::services;
mod telemetry;

pub use self::admission::{Admission, DEFAULT_MAX_IN_FLIGHT, admit, is_metered};
pub use self::auth::{Authenticator, Planes, RunnerIdentity};
pub use self::envelope::{CONTENT_TYPE_PROBLEM_JSON, ProblemResponse};
pub use self::request_id::{RequestId, UNKNOWN_REQUEST_ID};
pub use self::route::{Guard, Route, RouteClass, RouteMeta, Scopes};
pub use self::router::{Dependencies, ReadyInputs, Serving, ready_decision};
pub use self::server::{MAX_REQUEST_HEADER_BYTES, connection_builder};
pub use self::services::{SchedulePlane, Services, TenantSurface};
