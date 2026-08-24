//! What a client is told about an error, beyond its code.
//!
//! Mirrors `src/agentsfleetd/errors/error_entries.zig`, which pairs every
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

/// Reused title, so two entries cannot drift apart in their spelling.
const TITLE_REQUEST_FAILED: &str = "Request failed";

/// One entry per code this workspace declares, in `REGISTRY` order.
///
/// Every string is byte-identical to the Zig entry it mirrors, and
/// `test_entries_match_the_zig_registry` reads that file and fails if either
/// side moves — the same device the code list itself is held to.
const ENTRIES: &[Problem] = &[
    Problem {
        code: error_code::UUIDV7_INVALID_ID_SHAPE,
        status: 400,
        title: "Invalid identifier shape",
        hint: "The identifier is not a valid version 7 universally unique identifier (UUID).",
        user_message: None,
    },
    Problem {
        code: error_code::INVALID_REQUEST,
        status: 400,
        title: "Invalid request",
        hint: "The request body or parameters are invalid. Check the API documentation.",
        user_message: Some(
            "That request wasn't valid. Double-check the values you entered and try again.",
        ),
    },
    Problem {
        code: error_code::VAULT_DATA_INVALID,
        status: 400,
        title: "Secret data must be a non-empty JSON object",
        hint: "The body's 'data' field must be a JSON object with at least one key. Strings, arrays, scalars, and `{}` are rejected.",
        user_message: Some(
            "That secret needs at least one field. Enter it as a JSON object with one or more keys — not a bare string or list.",
        ),
    },
    Problem {
        code: error_code::INTERNAL_OPERATION_FAILED,
        status: 500,
        title: TITLE_REQUEST_FAILED,
        hint: "An internal operation failed. Check the err= field. If it continues, run 'agentsfleetd doctor'.",
        user_message: None,
    },
    Problem {
        code: error_code::INTERNAL_DB_UNAVAILABLE,
        status: 503,
        title: "Service unavailable",
        hint: "Check that DATABASE_URL is set and the database server is reachable. Run 'agentsfleetd doctor' to verify.",
        user_message: Some("A required service is unavailable. Try again shortly."),
    },
    Problem {
        code: error_code::INTERNAL_DB_QUERY,
        status: 500,
        title: TITLE_REQUEST_FAILED,
        hint: "A database query failed. Check the err= field and database logs.",
        user_message: Some("We couldn't finish that request. Try again shortly."),
    },
    Problem {
        code: error_code::STARTUP_MIGRATION_CHECK,
        status: 500,
        title: "Stored data is not ready",
        hint: "Database migration state could not be verified. Check database connectivity.",
        user_message: None,
    },
    Problem {
        code: error_code::STARTUP_REDIS_CONNECT,
        status: 500,
        title: "Event service unavailable",
        hint: "Redis is unreachable. Check that REDIS_URL_API is set and the Redis server accepts connections. Run 'agentsfleetd doctor' to verify.",
        user_message: None,
    },
    Problem {
        code: error_code::AUTH_INSUFFICIENT_SCOPE,
        status: 403,
        title: "Insufficient scope",
        hint: "Your token lacks a required scope. The error detail names it; see the [Scopes](/api-reference/scopes) reference.",
        user_message: Some(
            "You need an additional scope for that. Ask an agentsfleet admin to grant the scope this action requires.",
        ),
    },
    Problem {
        code: error_code::AUTH_UNAUTHORIZED,
        status: 401,
        title: "Unauthorized",
        hint: "Authentication required. Provide a valid Bearer token.",
        user_message: None,
    },
    Problem {
        code: error_code::AUTH_TOKEN_EXPIRED,
        status: 401,
        title: "Token expired",
        hint: "Your authentication token has expired. Re-authenticate.",
        user_message: None,
    },
    Problem {
        code: error_code::AUTH_UNAVAILABLE,
        status: 503,
        title: "Authentication service unavailable",
        hint: "Authentication service is temporarily unavailable. Retry shortly.",
        user_message: None,
    },
    Problem {
        code: error_code::AUTH_CLI_CREDENTIAL_REVOKED,
        status: 401,
        title: "Command-line credential revoked",
        hint: "This credential was revoked by a logout or by a newer login from this machine. Run `agentsfleet login` to get a new one.",
        user_message: None,
    },
    Problem {
        code: error_code::APIKEY_REVOKED,
        status: 401,
        title: "API key has been revoked",
        hint: "This key was revoked and can no longer authenticate. Mint a replacement with: POST /v1/api-keys",
        user_message: None,
    },
    Problem {
        code: error_code::RUN_INVALID_RUNNER_TOKEN,
        status: 401,
        title: "Invalid runner token",
        hint: "The Bearer runner_token is missing, malformed, or not recognized. Re-register the runner.",
        user_message: None,
    },
    Problem {
        code: error_code::RUN_ADMIN_STATE_BLOCKED,
        status: 401,
        title: "Runner admin state blocks access",
        hint: "This runner is cordoned, draining, drained, or revoked and cannot call the runner plane. Re-enroll the host to mint a fresh runner token.",
        user_message: None,
    },
];

/// Every entry, for the exhaustive walks the tests do.
///
/// `ENTRIES` is private because it is a lookup table rather than a list anyone
/// should iterate for its own sake; this is the read-only view the tests use to
/// prove it total against [`crate::error_code::REGISTRY`].
#[must_use]
pub const fn entries() -> &'static [Problem] {
    ENTRIES
}
