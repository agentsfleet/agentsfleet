//! Liveness and readiness, and the line between them.
//!
//! `handlers/health.zig` keeps these apart deliberately, and says why in its
//! own words: dependency checks live in `/readyz` because "mixing them here
//! would flap liveness during transient dependency outages". A liveness probe
//! that goes red when Postgres blinks gets the process KILLED and restarted,
//! which does nothing about Postgres and drops every request the instance was
//! serving. So `/healthz` answers for the process and nothing else.

use std::sync::Arc;

use afd_core::error_code;
use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use http::StatusCode;
use serde_json::json;

/// The name this service reports as.
const SERVICE: &str = "agentsfleetd";

/// The build this binary was cut from.
///
/// `CARGO_PKG_VERSION` rather than a build-script constant: `make
/// check-version` already holds it equal to the repository `VERSION`, so a
/// second source would be a second thing to keep in step.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The commit this binary was built from, when the build was told.
///
/// `build_options.git_commit` in the Zig daemon. A build that does not set it
/// reports `unknown` rather than failing: the field is for an operator
/// correlating a running process with a tree, and a cargo build from a
/// developer's working copy has no honest answer to give.
const COMMIT: &str = match option_env!("AFD_GIT_COMMIT") {
    Some(commit) => commit,
    None => "unknown",
};

/// What one readiness check found, one field per dependency.
///
/// `ReadyInputs` in `health.zig`. The fields stay separate all the way to the
/// wire because an operator's next action differs: a red database and a red
/// queue are different incidents, and collapsing them to one boolean means
/// reading the logs to learn which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadyInputs {
    /// Whether Postgres answered.
    pub database: bool,
    /// Whether Redis answered.
    pub queue: bool,
}

/// Whether an instance reporting `inputs` should take traffic.
///
/// `health.zig::readyDecision`, and pure for the same reason it is pure there:
/// the decision is the part worth testing, and it needs no datastore to test.
#[must_use]
pub const fn ready_decision(inputs: ReadyInputs) -> bool {
    inputs.database && inputs.queue
}

/// What `/readyz` consults.
///
/// The trait is the seam between §5 and §7: routing and the response shape are
/// settled here, and what it MEANS to reach Postgres and Redis is supplied by
/// whoever owns the pools. Generic rather than `dyn`, so the probe is a static
/// call and the trait needs no boxed future to stay object safe.
pub trait Dependencies: Send + Sync + std::fmt::Debug + 'static {
    /// Checks every dependency, reporting each separately.
    ///
    /// Never fails: an unreachable dependency is a `false` field, not an error.
    /// A probe that could itself error would give `/readyz` a third outcome
    /// beyond ready and not-ready, and an orchestrator has nothing to do with
    /// one.
    fn probe(&self) -> impl Future<Output = ReadyInputs> + Send;
}

/// `GET /healthz` — the process is up and answering.
///
/// Reads no state, touches no dependency. That is the whole contract.
pub(super) async fn healthz() -> Response {
    // `trace`: an orchestrator hits this every few seconds per instance, so at
    // any level a person leaves on it would be the loudest event in the log
    // and would say nothing. It exists for the case where someone needs to
    // prove the probe is arriving at all.
    tracing::trace!("liveness probed");
    Json(json!({
        "status": "ok",
        "service": SERVICE,
        "version": VERSION,
        "commit": COMMIT,
    }))
    .into_response()
}

/// `GET /readyz` — every dependency this instance needs is reachable.
///
/// Answers 503 when it is not, so an orchestrator takes the instance out of
/// rotation rather than restarting it — the process is fine, its dependencies
/// are not.
pub(super) async fn readyz<D: Dependencies>(State(dependencies): State<Arc<D>>) -> Response {
    let inputs = dependencies.probe().await;
    let ready = ready_decision(inputs);
    let status = if ready {
        tracing::trace!(
            database = inputs.database,
            queue = inputs.queue,
            "readiness probed"
        );
        StatusCode::OK
    } else {
        // Hoisted out of the macro: `tracing`'s `log` feature compiles a second
        // copy of every field expression, and llvm-cov attributes coverage to
        // the copy that never runs.
        let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
        // `warn`, not `error`: the instance is behaving correctly by refusing
        // traffic, and the incident belongs to whichever dependency is down.
        // An `error` here would page for someone else's outage twice.
        tracing::warn!(
            error_code = code,
            database = inputs.database,
            queue = inputs.queue,
            "instance is not ready — refusing traffic until dependencies answer"
        );
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status,
        Json(json!({
            "ready": ready,
            "database": inputs.database,
            "queue": inputs.queue,
        })),
    )
        .into_response()
}
