//! Every `afr_executor` integration test, in one test binary.
//!
//! Each test runs the executor and its client in-process over a real Unix
//! socket in a scratch directory, unsandboxed, so the suite runs on any
//! developer machine; the kernel lane runs the same executor inside the real
//! sandbox.

#[path = "files.rs"]
mod files;
#[path = "input.rs"]
mod input;
#[path = "lifecycle.rs"]
mod lifecycle;
#[path = "link.rs"]
mod link;
#[path = "orphans.rs"]
mod orphans;
#[path = "processes.rs"]
mod processes;
#[path = "protocol.rs"]
mod protocol;
#[path = "support.rs"]
mod support;
#[path = "terminal.rs"]
mod terminal;
