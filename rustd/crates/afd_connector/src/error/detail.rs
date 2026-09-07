//! The sentence each failure tells a caller, named so a suite can assert one
//! without respelling it.
//!
//! Every string is byte-identical to the `MSG_*` constant in
//! `errors/error_registry.zig` it mirrors, and to the copy
//! `afd_cron::error::detail` already carries. Two daemons answering one
//! incident with different prose read as two different bugs to whoever is
//! holding the page.

/// `problem_response.zig`'s `internalDbUnavailable` detail.
pub const DATABASE_UNAVAILABLE: &str = "Database unavailable";

/// `problem_response.zig`'s `internalDbError` detail.
pub const DATABASE_ERROR: &str = "Database error";

/// `problem_response.zig`'s `internalOperationError` detail.
pub const OPERATION_FAILED: &str = "Failed to complete the operation";

/// What a person is told when the provider could not be reached at all.
///
/// `callback.zig`'s `VENDOR_DEADLINE_FALLBACK`. It says nothing about which
/// leg failed, because the only thing the person can act on is the same
/// either way: nothing was connected, and trying again shortly may work.
pub const VENDOR_UNREACHABLE: &str = "Token exchange did not complete in time";

/// What a person is told when the provider answered and refused.
///
/// `callback.zig`'s `EXCHANGE_FAILED_FALLBACK`. Deliberately does not say
/// WHICH way it failed — a spent code, a mismatched redirect URI and a rotated
/// client secret are the same sentence to whoever pressed Connect, and the
/// difference is in the operator's log rather than in the answer.
pub const EXCHANGE_FAILED: &str = "Token exchange failed";

/// What a person is told when no single GitHub installation could be bound.
///
/// `github/callback.zig`'s ownership refusal sentence. One sentence for none
/// listed, several listed, a claim the token does not open and an
/// installation another workspace routes: every one of them is answered by
/// installing the App on the right account, or signing in as the account
/// that owns it, and connecting again — and which one it was is in the
/// operator's log rather than the answer.
pub const INSTALLATION_OWNERSHIP: &str = "GitHub installation ownership could not be verified";
