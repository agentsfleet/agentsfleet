//! What a client is told about an error, beyond its code.
//!
//! Mirrors what the retired daemon's `errors/error_entries.zig` paired: every
//! registry code with the status it answers, a title, a hint written for an
//! integrator, and — where the dashboard renders it — a sentence written for a
//! person. §5's `application/problem+json` envelope is assembled from exactly
//! these fields, so they live beside the codes rather than in the HTTP crate:
//! the status a code answers with is a property OF THE CODE, and two callers
//! answering different statuses for one code would be the bug this prevents.
//!
//! # Why the docs link is derived and not stored
//!
//! `docs_uri` is `ERROR_DOCS_BASE ++ code` in the Zig entries — a fact about
//! the documentation site's anchor scheme, not about the error. Deriving it
//! here means a code can never carry a link to a different code's anchor.
//!
//! # Why an unregistered code degrades rather than fails
//!
//! [`Problem::of`] answers [`Problem::UNKNOWN`] — a 500 — for a code with no
//! entry, exactly as `error_registry.lookup` returns its `UNKNOWN` entry. A
//! response is being written at that point and there is nothing better to do
//! than answer honestly. `test_every_declared_code_has_an_entry` is what stops
//! that fallback from ever being reached by a code this workspace declares.

use crate::error_code::{self, ErrorCode};

/// The documentation anchor every code's link is built from.
///
/// `error_entries.zig`'s `ERROR_DOCS_BASE` (RULE UFS).
pub const DOCS_BASE: &str = "https://docs.agentsfleet.net/api-reference/error-codes#";

/// Everything a client is told about one error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Problem {
    code: ErrorCode,
    status: u16,
    title: &'static str,
    hint: &'static str,
    user_message: Option<&'static str>,
}

impl Problem {
    /// The entry an unregistered code falls back to.
    ///
    /// Present so writing a response is total. Never reached by a code this
    /// workspace declares — a test proves that — and a 500 titled "Unknown
    /// error" is the honest answer if it ever were.
    pub const UNKNOWN: Self = Self {
        code: error_code::INTERNAL_OPERATION_FAILED,
        status: 500,
        title: "Unknown error",
        hint: "This error code is not registered. Report to the operator.",
        user_message: None,
    };

    /// The code this describes.
    #[must_use]
    pub const fn code(self) -> ErrorCode {
        self.code
    }

    /// The HTTP status the code answers with.
    ///
    /// A property of the CODE, which is the whole reason this table exists:
    /// `docs/AUTH.md` and the handlers both rely on `UZ-AUTH-022` being a 403
    /// and `UZ-AUTH-004` a 503, and a caller choosing the status per call site
    /// is how those drift.
    #[must_use]
    pub const fn status(self) -> u16 {
        self.status
    }

    /// The short human-readable summary.
    #[must_use]
    pub const fn title(self) -> &'static str {
        self.title
    }

    /// Guidance written for whoever is integrating against the API.
    #[must_use]
    pub const fn hint(self) -> &'static str {
        self.hint
    }

    /// A dashboard-safe sentence, where one is authored.
    ///
    /// `None` for the codes a person never sees — a runner-plane wire contract,
    /// a boot check, a command-line surface. The Zig side omits the field from
    /// the wire entirely rather than serializing a null, and §5's envelope does
    /// the same.
    #[must_use]
    pub const fn user_message(self) -> Option<&'static str> {
        self.user_message
    }

    /// Where a reader goes to learn more.
    ///
    /// Derived rather than stored, so a code cannot link to another's anchor.
    #[must_use]
    pub fn docs_uri(self) -> String {
        format!("{DOCS_BASE}{}", self.code.as_str())
    }

    /// The entry for `code`, or [`Problem::UNKNOWN`].
    #[must_use]
    pub fn of(code: ErrorCode) -> Self {
        ENTRIES
            .iter()
            .copied()
            .find(|entry| entry.code == code)
            .unwrap_or(Self::UNKNOWN)
    }
}

mod auth;
mod fleet;
mod integration;
mod request;

/// The families, in `REGISTRY` order — which is the order [`ENTRIES`] takes.
///
/// Split the same way [`crate::error_code`] is, so a code and the entry
/// describing it live in comparable files. `test_entries_match_the_zig_registry`
/// walks both against the Zig table, and a family that had drifted out of order
/// would fail there rather than in a reader's memory.
const FAMILIES: [&[Problem]; 4] = [
    self::request::REQUEST,
    self::auth::AUTH,
    self::fleet::FLEET,
    self::integration::INTEGRATION,
];

/// How many entries the families hold between them.
const TOTAL: usize = FAMILIES[0].len() + FAMILIES[1].len() + FAMILIES[2].len() + FAMILIES[3].len();

/// One entry per code this workspace declares, in `REGISTRY` order.
///
/// Flattened from [`FAMILIES`] at compile time rather than written out once
/// more: a table assembled from its parts cannot disagree with them, and
/// `Problem` is `Copy`, so the assembly costs nothing at run time. Every string
/// is byte-identical to the Zig entry it mirrors, and
/// `test_entries_match_the_zig_registry` reads that file and fails if either
/// side moves.
#[expect(
    clippy::indexing_slicing,
    reason = "every index is bounded by the loop condition above it, and the whole block is const-evaluated — an out-of-bounds here is a build failure, not a panic"
)]
const ENTRIES: [Problem; TOTAL] = {
    let mut flat = [Problem::UNKNOWN; TOTAL];
    let mut at = 0;
    let mut family = 0;
    while family < FAMILIES.len() {
        let entries = FAMILIES[family];
        let mut index = 0;
        while index < entries.len() {
            flat[at] = entries[index];
            at += 1;
            index += 1;
        }
        family += 1;
    }
    flat
};

/// Every entry, for the exhaustive walks the tests do.
///
/// [`ENTRIES`] is private because it is a lookup table rather than a list
/// anyone should iterate for its own sake; this is the read-only view the tests
/// use to prove it total against [`crate::error_code::REGISTRY`].
#[must_use]
pub const fn entries() -> &'static [Problem] {
    &ENTRIES
}
