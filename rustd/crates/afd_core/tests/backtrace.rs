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

use afd_core::error_code::{Coded, Logged};
use afd_core::limits::WorkerCount;

/// Set in the child so it runs the assertions instead of spawning again.
const CHILD_MARKER: &str = "AFD_BACKTRACE_CHILD";

/// What libtest prints when the child ran the one test it was asked for.
const RAN_ONE: &str = "1 passed";

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
        let told = err.told();
        assert_eq!(
            told.lines().count(),
            1,
            "a log's reason never carries the backtrace: {told}"
        );
        assert!(!told.starts_with('['), "nor the code: {told}");
        return;
    }

    // Parent: no RUST_BACKTRACE, so capture is skipped and Display stays on one
    // line — the cheap path that must not cost microseconds per error.
    assert_ne!(err.backtrace().status(), BacktraceStatus::Captured);
    assert_eq!(err.to_string().lines().count(), 1);

    rerun_with_backtraces("should_render_the_backtrace_only_when_the_environment_asks_for_one");
}

/// Two crates' shells, one wrapping the other, as `afr_supervisor` wraps an
/// `afr_sandbox` refusal: each is `error_shell!` expanded in a module of its own,
/// as it would be in a crate of its own.
mod nested {
    /// The engine: a shelled error over a foreign cause.
    pub(super) mod engine {
        use afd_core::error_code::{self, ErrorCode};

        /// What the engine refused.
        #[derive(Debug, thiserror::Error)]
        pub(crate) enum Kind {
            /// The engine's sentence over the kernel's errno.
            #[error("the lease's egress was refused: 257 addresses")]
            Refused {
                /// The kernel's answer.
                #[source]
                source: std::io::Error,
            },
        }

        afd_core::error_shell!(
            /// The engine's failure.
            pub struct Error(Kind);
        );

        impl Error {
            /// The engine's registry code.
            pub(crate) const fn code(&self) -> ErrorCode {
                match self.kind() {
                    Kind::Refused { .. } => error_code::INTERNAL_OPERATION_FAILED,
                }
            }
        }

        /// The engine refusing, over a kernel errno.
        pub(crate) fn refused() -> Error {
            Error::from(Kind::Refused {
                source: std::io::Error::from_raw_os_error(EINVAL),
            })
        }

        /// The errno the kernel answers with.
        pub(crate) const EINVAL: i32 = 22;
    }

    /// The supervisor: a shelled error whose cause is the engine's.
    pub(super) mod supervisor {
        use afd_core::error_code::{self, ErrorCode};

        use super::engine;

        /// What the supervisor could not do.
        #[derive(Debug, thiserror::Error)]
        pub(crate) enum Kind {
            /// The engine's failure as a cause beneath the supervisor's sentence.
            #[error("the lease's egress allowlist was refused")]
            Egress {
                /// The engine's reason.
                #[source]
                source: engine::Error,
            },
            /// The engine's failure as the supervisor's own sentence.
            #[error(transparent)]
            Passed {
                /// The engine's failure, told as this one.
                source: engine::Error,
            },
        }

        afd_core::error_shell!(
            /// The supervisor's failure.
            pub struct Error(Kind);
        );

        impl Error {
            /// The supervisor's registry code.
            pub(crate) const fn code(&self) -> ErrorCode {
                match self.kind() {
                    Kind::Egress { .. } | Kind::Passed { .. } => {
                        error_code::INTERNAL_DB_UNAVAILABLE
                    }
                }
            }
        }

        /// The engine's refusal beneath the supervisor's sentence.
        pub(crate) fn egress() -> Error {
            Error::from(Kind::Egress {
                source: engine::refused(),
            })
        }

        /// The engine's refusal as the supervisor's own sentence.
        pub(crate) fn passed() -> Error {
            Error::from(Kind::Passed {
                source: engine::refused(),
            })
        }
    }
}

/// A shelled cause beneath a shelled failure is told by its sentence: the log
/// line carries a code once, in `error_code`, and no backtrace, even when every
/// error in the chain captured one.
#[test]
fn should_tell_a_shelled_cause_without_its_code_or_backtrace() {
    let caused = nested::supervisor::egress();
    let passed = nested::supervisor::passed();
    let errno = std::io::Error::from_raw_os_error(nested::engine::EINVAL).to_string();

    if std::env::var(CHILD_MARKER).is_ok() {
        assert_eq!(caused.backtrace().status(), BacktraceStatus::Captured);
        assert!(
            caused.to_string().lines().count() > 1,
            "the capture this proof needs did not happen"
        );
    }

    let Logged { error_code, reason } = caused.logged();
    assert_eq!(error_code, caused.code().as_str());
    assert_eq!(
        reason,
        format!(
            "the lease's egress allowlist was refused: \
             the lease's egress was refused: 257 addresses: {errno}"
        )
    );
    assert_eq!(
        passed.told(),
        format!("the lease's egress was refused: 257 addresses: {errno}")
    );
    for told in [&reason, &passed.told()] {
        assert!(!told.contains("[UZ-"), "no code in the reason: {told}");
        assert_eq!(
            told.lines().count(),
            1,
            "no backtrace in the reason: {told}"
        );
    }

    if std::env::var(CHILD_MARKER).is_err() {
        rerun_with_backtraces("should_tell_a_shelled_cause_without_its_code_or_backtrace");
    }
}

/// Runs `test` again in a child with `RUST_BACKTRACE=1`, and fails unless the
/// child ran that one test and it passed.
///
/// The name is the test's path inside the binary: this file is a module of
/// `core_suite`, and a bare name under `--exact` matches nothing, which a child
/// reports as a pass.
fn rerun_with_backtraces(test: &str) {
    let path = module_path!().split_once("::").map_or_else(
        || test.to_owned(),
        |(_suite, module)| format!("{module}::{test}"),
    );
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &path, "--nocapture"])
        .env("RUST_BACKTRACE", "1")
        .env(CHILD_MARKER, "1")
        .output()
        .expect("re-executing the test binary must work");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains(RAN_ONE),
        "child run of {path} failed or ran nothing:\n{stdout}"
    );
}
