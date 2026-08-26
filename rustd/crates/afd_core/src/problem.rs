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
        code: error_code::RUN_STALE_FENCING_TOKEN,
        // 409, matching the Zig entry's `.conflict`. The word is exact: two
        // runners each hold a lease they believe is live, and the fence is what
        // settles which one is. Not a 403 — nothing about the credential is
        // wrong — and not a 410, because the resource is very much still there,
        // owned by somebody else.
        status: 409,
        title: "Stale fencing token",
        hint: "The lease was reclaimed by a newer holder. This report is rejected; the current holder's result wins.",
        // Not dashboard-facing: this rides the runner-to-control-plane wire
        // contract, and the Zig entry carries the same reachability note.
        user_message: None,
    },
    Problem {
        code: error_code::RUN_LEASE_NOT_FOUND,
        status: 404,
        title: "Lease not found",
        hint: "No active lease matches this lease_id for the presenting runner; it may have expired, been reclaimed, or never existed.",
        user_message: None,
    },
    Problem {
        code: error_code::RUN_ADMIN_STATE_BLOCKED,
        status: 401,
        title: "Runner admin state blocks access",
        hint: "This runner is cordoned, draining, drained, or revoked and cannot call the runner plane. Re-enroll the host to mint a fresh runner token.",
        user_message: None,
    },
    Problem {
        code: error_code::RUN_LEASE_EXCEEDED_MAX_RUNTIME,
        // 409 like the lost verdict beside it, and the pair is the reason both
        // codes exist: the STATUS cannot tell a runner whether its result is
        // still wanted, so the code has to.
        status: 409,
        title: "Lease exceeded max runtime",
        hint: "The lease reached its maximum runtime and cannot renew. The runner stops the child and reports any result.",
        user_message: None,
    },
    Problem {
        code: error_code::RUN_LEASE_LOST,
        status: 409,
        title: "Lease lost",
        hint: "The lease moved to another runner before renewal. The former runner must stop its child.",
        user_message: None,
    },
    Problem {
        code: error_code::RUN_LEASE_RENEWAL_NO_CREDITS,
        // 402, and load-bearing for the reason the entry below it is: the stock
        // runner classifies a renew refusal by status AND code. Both 402s stop
        // the run; the code is what says which pool ran dry, and therefore
        // whether an operator tops up a balance or edits a ceiling.
        status: 402,
        title: "Lease renewal blocked: no credits",
        hint: "The tenant balance cannot cover another run slice. The lease does not renew, and the run stops cleanly.",
        user_message: None,
    },
    Problem {
        code: error_code::RUNNER_NOT_FOUND,
        status: 404,
        title: "Runner not found",
        hint: "No runner matches this runner_id. Verify the platform admin minted the runner before mutating it.",
        user_message: Some(
            "We couldn't find that runner. It may have been removed — refresh the list.",
        ),
    },
    Problem {
        code: error_code::RUN_BUDGET_EXCEEDED,
        // 402, and the status is load-bearing rather than decorative: the stock
        // runner classifies a renew refusal by BOTH status and code, and
        // `control_plane_client_test.zig` pins that a UZ-RUN-015 arriving on
        // any other terminal status is NOT a budget breach. A 403 here would
        // leave the runner treating an exhausted ceiling as an auth failure.
        status: 402,
        title: "Lease renewal blocked: fleet budget exhausted",
        hint: "The fleet reached its daily_dollars or monthly_dollars limit from `TRIGGER.md`, so the run stops. The tenant balance is fine; this is the fleet's own budget.",
        // Not dashboard-facing: this rides the runner-to-control-plane wire
        // protocol, and the Zig entry carries the same reachability note.
        user_message: None,
    },
    Problem {
        code: error_code::AGENTSFLEET_CREDENTIAL_MISSING,
        // 424, matching the Zig entry's `.failed_dependency`. The fleet's own
        // request is well-formed; what is missing is a credential it depends
        // on, which is the distinction this status exists to make.
        status: 424,
        title: "Fleet credential missing",
        hint: "A required credential is not in the vault. Add it with: `agentsfleet secret create <NAME>`",
        // Not dashboard-facing, and the Zig entry carries the same reachability
        // note: this is a CLI and API-key surface, and on the lease path it is
        // logged rather than rendered at all.
        user_message: None,
    },
    Problem {
        code: error_code::FLEET_BUNDLE_INVALID,
        status: 400,
        title: "Invalid Fleet Bundle",
        hint: "The supplied Fleet Bundle is missing `SKILL.md` or contains unsafe, oversized, or malformed files.",
        user_message: Some(
            "That Fleet Bundle isn't valid. It's missing `SKILL.md`, or has an unsafe or oversized file. Check the source and try again.",
        ),
    },
    Problem {
        code: error_code::FLEET_BUNDLE_NOT_FOUND,
        status: 404,
        title: "Fleet Bundle not found",
        hint: "No installable library entry or stored snapshot matches the request in this workspace.",
        user_message: Some(
            "We couldn't find that Fleet Bundle. It may not be installed in this workspace yet — check the Fleet library.",
        ),
    },
    Problem {
        code: error_code::FLEET_BUNDLE_FETCH_FAILED,
        status: 502,
        title: "Fleet Bundle fetch failed",
        hint: "The Fleet Bundle source could not be fetched from GitHub. The repository may be missing or private, or GitHub may be unreachable. Verify the source reference and retry.",
        user_message: Some(
            "We couldn't fetch that Fleet Bundle from GitHub. Check the source and try again.",
        ),
    },
    Problem {
        code: error_code::FLEET_BUNDLE_STORAGE_UNAVAILABLE,
        status: 503,
        title: "Fleet Bundle storage unavailable",
        hint: "Snapshot storage is not configured or is unavailable, so the validated bundle could not be stored. Retry later or contact the operator.",
        user_message: Some("We couldn't store your Fleet Bundle right now. Try again shortly."),
    },
    Problem {
        code: error_code::CRED_INTEGRATION_NOT_CONNECTED,
        status: 404,
        title: "Integration not connected",
        hint: "No connected integration matches this id in the fleet's workspace. Connect it from the dashboard first.",
        user_message: Some(
            "That integration isn't connected. Connect it from the Integrations page, then try again.",
        ),
    },
    Problem {
        code: error_code::CRED_BROKER_NOT_CONFIGURED,
        status: 503,
        title: "Credential broker not configured",
        hint: "The on-demand credential broker is not configured on this deployment. An operator must set it up before runners can mint credentials.",
        // Runner-only mint endpoint; the Zig entry carries the same
        // reachability note, and nothing in `ui/packages/app` fetches it.
        user_message: None,
    },
    Problem {
        code: error_code::GH_RECONNECT_REQUIRED,
        status: 409,
        title: "GitHub App reconnect required",
        hint: "The GitHub App installation was uninstalled or revoked, so no token can be minted. Reconnect GitHub from the dashboard.",
        // Surfaced to the agent as a tool failure, not to a dashboard fetch.
        user_message: None,
    },
    Problem {
        code: error_code::GH_MINT_FAILED,
        status: 502,
        title: "GitHub token mint failed",
        hint: "GitHub did not return an installation token. Retry shortly; if it continues, check GitHub status and the App configuration.",
        user_message: None,
    },
    Problem {
        code: error_code::GRANT_NOT_FOUND,
        status: 403,
        title: "No integration grant for service",
        hint: "This fleet has no approved grant for the target service. Check it with `GET /v1/workspaces/{ws}/fleets/{id}/integration-grants` and resolve its approval.",
        // Runner-only mint and lease gate.
        user_message: None,
    },
    Problem {
        code: error_code::CONNECTOR_OAUTH_EXCHANGE_FAILED,
        status: 502,
        title: "Connector OAuth exchange failed",
        hint: "The connector's OAuth exchange was rejected. Start the connect again; if it repeats, check the provider app credentials and redirect URL.",
        user_message: Some(
            "That connection didn't go through. Try connecting again from the dashboard.",
        ),
    },
    Problem {
        code: error_code::REPAIR_WRITE_UNAPPROVED,
        status: 403,
        title: "Write mint requires an approved gate",
        hint: "No repository-write approval was answered for this event, so no write-scoped token issues. The run continues read-only.",
        user_message: None,
    },
    Problem {
        code: error_code::REPAIR_BINDING_DRIFT,
        status: 403,
        title: "Fleet binding changed since approval",
        hint: "The fleet's repository binding no longer matches the approved card. Re-raise the approval so a human sees the current reach.",
        user_message: None,
    },
    Problem {
        code: error_code::REPAIR_SPEND_EXHAUSTED,
        status: 403,
        title: "Write request allowance exhausted",
        hint: "This approval already funded 32 write-credential requests. Answer a new repository-write approval first.",
        user_message: None,
    },
    Problem {
        code: error_code::API_BACKPRESSURE,
        status: 429,
        title: "Too many requests",
        hint: "The API is at its request limit. Wait for the Retry-After delay, then retry.",
        // No dashboard sentence, and the Zig entry says why in its own
        // reachability note: a shed happens before routing, so nothing that
        // renders a problem page is ever reached to render this one.
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
