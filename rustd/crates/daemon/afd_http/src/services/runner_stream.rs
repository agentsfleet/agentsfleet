//! HTTP states opening; the native gateway owns the session after that boundary.

use afd_auth::principal::Runner;
use axum::{extract::Request, response::Response};

/// The runner opening served through the common router and its guard.
pub trait RunnerStreams: Send + Sync + std::fmt::Debug + 'static {
    /// Open or attach one lane on an authenticated native connection.
    fn open(&self, runner: Runner, request: Request) -> impl Future<Output = Response> + Send;
}
