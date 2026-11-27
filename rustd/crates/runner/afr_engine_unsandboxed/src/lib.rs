//! The debug engine: no sandbox, for the lanes that run where no microVM
//! can.
//!
//! [`UnsandboxedEngine`] serves the real guest agent in-process, over a real
//! socket, in a scratch directory, so the supervisor and the daemon are
//! proven end to end on any developer machine; a release build refuses to
//! construct it. Behind `test-util`, [`RecoveryDrillEngine`] wraps it with a
//! named toolbox and a quiet tree, so a recovery drill can checkpoint and
//! restore through it.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod error;

mod unsandboxed;

pub use self::error::{Error, Result};
/// The unconfined engine a recovery drill checkpoints and restores a
/// workspace through (M-TEST-UTIL).
#[cfg(feature = "test-util")]
pub use self::unsandboxed::RecoveryDrillEngine;
pub use self::unsandboxed::UnsandboxedEngine;
