//! Stream metadata and state belong to the gateway; the common guard already proved the principal.

use std::sync::Arc;
use axum::{extract::{Request, State}, response::Response};
use afd_http::{auth::RunnerIdentity, services::{RunnerStreams as _, Services}};

pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>, RunnerIdentity(runner): RunnerIdentity, request: Request,
) -> Response {
    services.runner_streams().open(runner, request).await
}
