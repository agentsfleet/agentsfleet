//! The scaffolding every crate-level error shares, generated once.
//!
//! # What was duplicated, and what stays the crate's
//!
//! Six crates carried a byte-identical hull around their error: the
//! `struct Error { inner: Box<Inner> }` with its captured backtrace, the
//! `From<ErrorKind>` that is the one place a kind becomes an error, the
//! `Display` that renders `[CODE] message` plus the backtrace when one was
//! captured, and the `source()` that skips the kind (`RUST_ERROR_STANDARD`
//! rule 4 — the kind is not a CAUSE of this error, it IS this error). Every
//! copy was the same because none of it depends on what went wrong.
//!
//! What DOES depend on what went wrong stays in the crate, ungenerated: the
//! `ErrorKind` enum, the `code()`/`detail()` mapping, and the `is_*` accessors
//! a caller branches on. A macro that generated those would be a second place
//! to look for a crate's failure vocabulary; this one generates only the hull,
//! which a reader never needs to look at.
//!
//! # Why a `macro_rules!` and not a derive
//!
//! A derive runs on ONE item and could produce the impls but not the sibling
//! types (`Inner`, the alias). More importantly a proc-macro crate would be a
//! new compilation unit every tier-0 consumer waits on; `macro_rules!` costs
//! nothing at build time and expands in the crate that calls it.

/// Generates the hull of a crate-level error: the boxed struct, the backtrace
/// capture, `Display`, `source()`, and the `Result` alias.
///
/// The calling crate declares `ErrorKind` (a `thiserror::Error` enum), its own
/// `pub type Result<T, E = Error>` alias, and `code()` on `Error` — `Display`
/// calls the last, so the build fails until the mapping exists, which is the
/// order the standard wants the two written in anyway.
///
/// The alias is deliberately NOT generated. `RUST_ERROR_STANDARD` rule 1 exists
/// so a reader never has to check which error a signature returns, and an alias
/// that only appears after macro expansion is one they cannot see — which is
/// also exactly what `audits/rust-err.sh` reports. One visible line per crate
/// beats a hidden one.
///
/// ```ignore
/// afd_core::error_shell!(
///     /// A vault failure, with the backtrace of where it was raised.
///     pub struct Error(ErrorKind);
/// );
/// ```
#[macro_export]
macro_rules! error_shell {
    (
        $(#[$doc:meta])*
        pub struct $error:ident($kind:ty);
    ) => {
        $(#[$doc])*
        #[derive(Debug)]
        pub struct $error {
            inner: Box<ErrorShellInner>,
        }

        /// Boxed so the `Result` stays pointer-sized on the `Ok` path — the
        /// backtrace alone would otherwise put the whole capture inline in
        /// every return value.
        #[derive(Debug)]
        struct ErrorShellInner {
            kind: $kind,
            backtrace: std::backtrace::Backtrace,
        }

        impl $error {
            /// What actually went wrong, for the accessors the crate declares.
            #[allow(
                dead_code,
                reason = "some crates read the kind only through their own accessors"
            )]
            pub(crate) const fn kind(&self) -> &$kind {
                &self.inner.kind
            }

            /// Where this failure was raised — populated only when the
            /// environment asked for one (`RUST_BACKTRACE`), so the common
            /// path stays cheap.
            ///
            /// No `#[must_use]`: `Backtrace` already carries one, and the
            /// doubled attribute is only VISIBLE from inside this crate —
            /// clippy suppresses external-macro lints, so every other
            /// expansion hid it.
            pub fn backtrace(&self) -> &std::backtrace::Backtrace {
                &self.inner.backtrace
            }
        }

        /// The one place a kind becomes an error, so every raise captures a
        /// backtrace.
        impl From<$kind> for $error {
            fn from(kind: $kind) -> Self {
                Self {
                    inner: Box::new(ErrorShellInner {
                        kind,
                        backtrace: std::backtrace::Backtrace::capture(),
                    }),
                }
            }
        }

        // The captured-backtrace branch below is proven by
        // `tests/backtrace.rs`, which re-executes the test binary with
        // `RUST_BACKTRACE=1` because the variable is read once per PROCESS.
        // Coverage does not follow that child, so the line reads as unhit
        // while a passing test proves it is not. It was equally unhit before
        // this hull was generated, when every crate wrote the same branch by
        // hand; generating it moved one uncovered line rather than adding
        // seven. Making it reachable in-process would mean restructuring the
        // capture for the tool's benefit, which that test file declines to do
        // for the same reason.
        impl std::fmt::Display for $error {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "[{}] {}", self.code().as_str(), self.inner.kind)?;
                if self.inner.backtrace.status() == std::backtrace::BacktraceStatus::Captured {
                    write!(f, "\n{}", self.inner.backtrace)?;
                }
                Ok(())
            }
        }

        impl std::error::Error for $error {
            /// The failure beneath this one, skipping our own kind.
            ///
            /// `Display` already renders the kind's message, so returning the
            /// kind would make a chain walker print the same sentence twice
            /// before reaching anything new. The kind is not a CAUSE of this
            /// error, it IS this error (`RUST_ERROR_STANDARD` rule 4).
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                std::error::Error::source(&self.inner.kind)
            }
        }
    };
}

/// Lifts each named source type into its `ErrorKind` variant, so `?` needs no
/// arm at a call site (`RUST_ERROR_STANDARD` rule 2).
///
/// Per source type rather than one blanket over `Into<ErrorKind>`, which would
/// collide with the standard library's reflexive `From<T> for T`.
///
/// ```ignore
/// afd_core::error_lifts!(Error, ErrorKind:
///     afd_db::Error => Datastore,
///     sqlx::Error => Driver,
/// );
/// ```
#[macro_export]
macro_rules! error_lifts {
    ($error:ident, $kind:ident: $($source:ty => $variant:ident),+ $(,)?) => {
        $(impl From<$source> for $error {
            fn from(source: $source) -> Self {
                $error::from($kind::$variant { source })
            }
        })+
    };
}
