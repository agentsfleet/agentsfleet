//! Newest-message decisions gate native edits, independently of chat output and usage.

mod tls;
#[path = "../../../../../afd_runner/tests/integration_runner_plane/decision_fixture.rs"]
mod decision_model_fixture;
mod server;
mod fixture;
mod outcomes;
mod fallback;
