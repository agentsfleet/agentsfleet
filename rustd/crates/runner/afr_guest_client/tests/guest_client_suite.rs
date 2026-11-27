//! Every `afr_guest_client` integration test, in one test binary, against the
//! real guest agent (`afg_agent`, a dev-dependency).
//!
//! Each test runs the guest agent and its client in-process over a real Unix
//! socket in a scratch directory, unsandboxed, so the suite runs on any
//! developer machine; the kernel lane runs the same guest agent inside the real
//! sandbox.

#[path = "files.rs"]
mod files;
#[path = "inner.rs"]
mod inner;
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
#[path = "search.rs"]
mod search;
#[path = "support.rs"]
mod support;
#[path = "terminal.rs"]
mod terminal;
#[path = "tree.rs"]
mod tree;
