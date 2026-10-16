//! Runner-plane HTTP regression suite.

mod harness;

#[path = "integration_runner_heartbeat.rs"]
mod integration_runner_heartbeat;
#[path = "integration_runner_rotation.rs"]
mod integration_runner_rotation;
mod runner_plane;
