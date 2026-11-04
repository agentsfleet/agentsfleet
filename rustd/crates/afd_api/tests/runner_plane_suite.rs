//! Runner-plane HTTP regression suite.

mod harness;

#[cfg(feature = "test-util")]
#[path = "../../afd_runner/tests/integration_runner_plane/decision_fixture.rs"]
#[expect(
    clippy::expect_used,
    reason = "shared catalogue fixture fails on unmet prerequisites"
)]
mod decision_model_fixture;

#[cfg(feature = "test-util")]
#[path = "../../afd_fleet/tests/integration_tasks/decisions/reply.rs"]
mod decision_reply;

#[path = "integration_runner_heartbeat.rs"]
mod integration_runner_heartbeat;
#[path = "integration_runner_rotation.rs"]
mod integration_runner_rotation;
mod runner_plane;
