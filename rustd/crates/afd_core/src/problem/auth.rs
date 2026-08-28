//! What a client is told about who they are, and what they may hold.
//!
//! Several statuses here are load-bearing rather than decorative — `docs/AUTH.md`
//! and the handlers both rely on `UZ-AUTH-022` being a 403 and `UZ-AUTH-004` a
//! 503, which is what this table exists to keep true in one place.

use super::Problem;
use crate::error_code;

/// This family's entries, in `REGISTRY` order.
pub(super) const AUTH: &[Problem] = &[
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
        code: error_code::AUTH_FORBIDDEN,
        status: 403,
        title: "Forbidden",
        hint: "Access denied. Check that your API key has the required role.",
        user_message: Some(
            "You need operator access for that. Ask a tenant operator or admin to manage API keys.",
        ),
    },
    Problem {
        code: error_code::SESSION_NOT_FOUND,
        status: 404,
        title: "Session not found",
        hint: "Session was not found. It may have expired or been invalidated.",
        // The whole device-flow family renders in a terminal rather than in
        // the dashboard, so none of these carries a sentence written for a
        // person — the Zig entries say the same in their reachability notes.
        user_message: None,
    },
    Problem {
        code: error_code::SESSION_EXPIRED,
        status: 401,
        title: "Session expired",
        hint: "Your session has expired. Please sign in again.",
        user_message: None,
    },
    Problem {
        code: error_code::VERIFICATION_FAILED,
        status: 400,
        title: "Verification code did not match",
        hint: "The 6-digit code does not match the one shown in your browser. Re-enter it and try again.",
        user_message: None,
    },
    Problem {
        code: error_code::SESSION_CONSUMED,
        status: 410,
        title: "Login session already consumed",
        hint: "This login session has already been consumed. Start over with `agentsfleet login`.",
        user_message: None,
    },
    Problem {
        code: error_code::SESSION_ABORTED,
        status: 410,
        title: "Login session aborted",
        hint: "This login session was aborted: too many wrong codes, a cancel, or a newer session. Start over with `agentsfleet login`.",
        user_message: None,
    },
    Problem {
        code: error_code::SESSION_NOT_APPROVED,
        status: 409,
        title: "Login session not approved",
        hint: "This login session is not approved yet. Approve it in your browser, then submit the code.",
        user_message: None,
    },
    Problem {
        code: error_code::SESSION_ALREADY_APPROVED,
        status: 409,
        title: "Login session already approved",
        hint: "This login session has already been approved. Do not call /approve a second time.",
        user_message: None,
    },
    Problem {
        code: error_code::INVALID_PUBLIC_KEY,
        status: 400,
        title: "Invalid command-line public key",
        hint: "The supplied public_key is malformed. Expect base64url-encoded P-256 SubjectPublicKeyInfo.",
        user_message: None,
    },
    Problem {
        code: error_code::INVALID_TOKEN_NAME,
        status: 400,
        title: "Invalid token name",
        hint: "token_name must contain 1 to 64 characters from space through tilde.",
        user_message: None,
    },
    Problem {
        code: error_code::INVALID_VERIFICATION_CODE,
        status: 400,
        title: "Invalid verification code shape",
        hint: "verification_code must contain exactly 6 decimal digits.",
        user_message: None,
    },
    Problem {
        code: error_code::INVALID_CIPHERTEXT,
        status: 400,
        title: "Invalid ciphertext",
        hint: "ciphertext is missing or empty. Expect a base64url-encoded AES-256-GCM output.",
        user_message: None,
    },
    Problem {
        code: error_code::INVALID_NONCE,
        status: 400,
        title: "Invalid nonce",
        hint: "nonce is missing, empty, or the wrong length. Expect a base64url-encoded 12-byte value.",
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
        code: error_code::AUTH_CLI_CREDENTIAL_NOT_FOUND,
        status: 404,
        title: "Command-line credential not found",
        hint: "You have no live credential with that identifier. It may be revoked, or it may not be yours.",
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
        code: error_code::APIKEY_NOT_FOUND,
        status: 404,
        title: "API key not found",
        hint: "No API key matches the supplied id for this tenant. Verify the id with: GET /v1/api-keys",
        user_message: Some(
            "We couldn't find that API key. It may have already been deleted — refresh the list.",
        ),
    },
    Problem {
        code: error_code::APIKEY_NAME_TAKEN,
        status: 409,
        title: "Key name already exists in this tenant",
        hint: "key_name must be unique per tenant. Pick a different name or revoke the existing key first.",
        user_message: Some(
            "An API key with that name already exists. Pick a different name for this tenant.",
        ),
    },
    Problem {
        code: error_code::APIKEY_ALREADY_REVOKED,
        status: 409,
        title: "API key is already revoked",
        hint: "This key is already revoked. No further action is required.",
        user_message: Some(
            "That API key is already revoked. Refresh the list to see its current state.",
        ),
    },
    Problem {
        code: error_code::APIKEY_READONLY_FIELD,
        status: 409,
        title: "active cannot be set to true; mint a new key instead",
        hint: "Re-activation is not supported. Create a new key via POST /v1/api-keys and revoke the old one.",
        user_message: Some("A revoked key can't be reactivated. Mint a new key instead."),
    },
    Problem {
        code: error_code::APIKEY_MUST_REVOKE_FIRST,
        status: 409,
        title: "Active API key must be revoked before deletion",
        hint: "Revoke the key first with `PATCH /v1/api-keys/{id}` body `{\"active\": false}`, then retry DELETE.",
        user_message: Some("This key is still active. Revoke it first, then delete it."),
    },
];
