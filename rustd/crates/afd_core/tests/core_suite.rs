//! Every `afd_core` test file, in one test binary.
//!
//! One binary rather than nine: cargo runs test BINARIES serially and the
//! tests inside one binary in parallel, so nine binaries were nine serial
//! stretches that each re-paid process start and dynamic linking — measured
//! at 309 ms of overhead apiece on this lane. Nothing here touches a
//! datastore, so the files are pure modules with no shared state to guard.
//! This is the shape `afd_api` already uses for its four planes.

#[path = "backtrace.rs"]
mod backtrace;
#[path = "clock.rs"]
mod clock;
#[path = "env.rs"]
mod env;
#[path = "error_code.rs"]
mod error_code;
#[path = "id.rs"]
mod id;
#[path = "limits.rs"]
mod limits;
#[path = "problem.rs"]
mod problem;
#[path = "workspace.rs"]
mod workspace;
