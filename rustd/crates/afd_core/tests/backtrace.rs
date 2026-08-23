//! The backtrace half of `Error`: captured only when asked, rendered when captured.
//!
//! `Backtrace::capture()` reads `RUST_BACKTRACE` once per PROCESS and caches the
//! answer, so both branches cannot be exercised in one test binary. This file
//! re-executes itself as a child with the variable set — the honest way to prove
//! the rendering actually happens, rather than restructuring production code to
//! make a branch reachable from a test.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test target: a failed re-exec is an unmet precondition"
)]

use std::backtrace::BacktraceStatus;

use afd_core::limits::WorkerCount;

/// Set in the child so it runs the assertions instead of spawning again.
const CHILD_MARKER: &str = "AFD_BACKTRACE_CHILD";

#[test]
fn should_render_the_backtrace_only_when_the_environment_asks_for_one() {
    let err = WorkerCount::new(0).unwrap_err();

    if std::env::var(CHILD_MARKER).is_ok() {
        // Child: RUST_BACKTRACE=1, so capture succeeds and Display appends it.
        assert_eq!(err.backtrace().status(), BacktraceStatus::Captured);
        let rendered = err.to_string();
        assert!(rendered.starts_with("[UZ-REQ-001]"), "{rendered}");
        assert!(
            rendered.lines().count() > 1,
            "a captured backtrace must be rendered, got: {rendered}"
        );
        return;
    }

    // Parent: no RUST_BACKTRACE, so capture is skipped and Display stays on one
    // line — the cheap path that must not cost microseconds per error.
    assert_ne!(err.backtrace().status(), BacktraceStatus::Captured);
    assert_eq!(err.to_string().lines().count(), 1);

    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "should_render_the_backtrace_only_when_the_environment_asks_for_one",
            "--nocapture",
        ])
        .env("RUST_BACKTRACE", "1")
        .env(CHILD_MARKER, "1")
        .output()
        .expect("re-executing the test binary must work");
    assert!(
        output.status.success(),
        "child run failed:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}
