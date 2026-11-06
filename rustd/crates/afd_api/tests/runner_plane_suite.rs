//! Runner-plane HTTP regression suite.

mod harness;

#[cfg(feature = "test-util")]
#[path = "../../afd_fleet/tests/integration_tasks/decisions/reply.rs"]
mod decision_reply;

#[path = "integration_runner_heartbeat.rs"]
mod integration_runner_heartbeat;
#[path = "integration_runner_rotation.rs"]
mod integration_runner_rotation;
mod runner_plane;
