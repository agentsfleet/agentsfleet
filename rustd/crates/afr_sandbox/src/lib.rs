//! Sandbox engines: where a lease's tool calls run.
//!
//! [`Engine`] builds one [`Sandbox`] per lease and the sandbox hands back the
//! [`afr_executor::Executor`] running inside it. The interface assumes no file
//! system shared with the host — the workspace is a block image, the toolbox an
//! image file, the executor a Unix socket — so a microVM engine attaches the
//! same artifacts (`docs/architecture/runner_execution.md` §Sandbox engines).

pub mod error;

mod engine;
mod probe;

pub use self::engine::{
    DEFAULT_CPU_MILLIS, DEFAULT_DISK_BYTES, DEFAULT_MEMORY_BYTES, DEFAULT_PIDS, Engine, Limits,
    Sandbox, SandboxRequest,
};
pub use self::error::{Error, Result};
pub use self::probe::{HostProbe, Kvm};
