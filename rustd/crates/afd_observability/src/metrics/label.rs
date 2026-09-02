//! The values a closed label may take, and nothing else.
//!
//! # Why these are types rather than strings at the call site
//!
//! Every one of these is a dimension with a fixed set of members, and the
//! census turns that set into a number — the series ceiling the SDK enforces.
//! A producer passing a string could write a value outside the set, and the
//! consequence is not a bad label: it is one more series than the ceiling
//! admits, which makes the SDK fold live data into `otel.metric.overflow`. So
//! the set is a type, the ceiling is derived from it in a test, and a member
//! added without raising the ceiling fails the build rather than the export.
//!
//! # Absence is not a member
//!
//! Where the Zig carries a `not_applicable` member, the Rust carries an
//! `Option` and records nothing for `None`. Both express "no cache decision was
//! made"; only one of them stops that non-decision from occupying a series.

pub mod cost;
pub mod fleet;
pub mod http;
pub mod library;

#[cfg(test)]
mod tests;

/// Declares a closed label set: the members, their wire spellings, and the
/// list a ceiling is derived from.
///
/// A macro because the three have to stay in step and there is no way to write
/// them once by hand. `ALL` in particular is what the census ceiling is graded
/// against, so a member added without it would silently understate the budget —
/// the exact drift this whole module exists to prevent.
macro_rules! closed_set {
    (
        $(#[$outer:meta])*
        $name:ident {
            $( $(#[$inner:meta])* $variant:ident => $label:expr ),+ $(,)?
        }
    ) => {
        $(#[$outer])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name {
            $( $(#[$inner])* $variant ),+
        }

        impl $name {
            /// Every member this build can write.
            ///
            /// The census ceiling for a family carrying this label must be at
            /// least the product of this length and its other labels'.
            pub const ALL: &'static [Self] = &[ $( Self::$variant ),+ ];

            /// The label value, byte-exact.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $( Self::$variant => $label ),+
                }
            }
        }
    };
}

pub(crate) use closed_set;
