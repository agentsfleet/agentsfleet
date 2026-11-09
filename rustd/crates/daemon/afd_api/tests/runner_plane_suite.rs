//! Runner-plane HTTP regression suite.

mod harness;

#[cfg(feature = "test-util")]
use afd_fleet::test_util::decision as decision_reply;

#[path = "integration_runner_rotation.rs"]
mod integration_runner_rotation;
mod runner_plane;
