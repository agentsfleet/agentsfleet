//! What a client is told about a request's own faults, and this instance's.
//!
//! Split from [`super`] along the same line [`crate::error_code`] splits on, so
//! a family's code and the entry describing it sit in comparable files.

use super::Problem;
use crate::error_code;

/// Reused title, so two entries here cannot drift apart in their spelling.
///
/// Lives with the family that uses it: the other three name their own failures,
/// and a shared constant they never reach would read as one they might.
const TITLE_REQUEST_FAILED: &str = "Request failed";

/// Reused title, shared with the provider family's own missing-secret refusal
/// exactly as `error_entries.zig` shares `S_TITLE_SECRET_NOT_FOUND`.
const TITLE_SECRET_NOT_FOUND: &str = "Secret not found";

/// This family's entries, in `REGISTRY` order.
pub(super) const REQUEST: &[Problem] = &[
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
        code: error_code::PAYLOAD_TOO_LARGE,
        status: 413,
        title: "Payload too large",
        hint: "Request body exceeds the maximum allowed size.",
        user_message: None,
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
        code: error_code::VAULT_DATA_TOO_LARGE,
        status: 400,
        title: "Secret data too large",
        hint: "Stringified secret data exceeds 4 KiB. Compose the secret from fewer or shorter fields.",
        user_message: Some(
            "That secret is too large. Keep it under 4 KiB. Trim or shorten the fields.",
        ),
    },
    Problem {
        code: error_code::SECRET_NOT_FOUND,
        status: 404,
        title: TITLE_SECRET_NOT_FOUND,
        hint: "No secret matches this name in the workspace. List the workspace secrets to find a valid name, or create it first.",
        user_message: Some(
            "We couldn't find that secret. It may have already been deleted — refresh the list.",
        ),
    },
    Problem {
        code: error_code::SECRET_REFERENCED_BY_MODEL_ENTRIES,
        status: 409,
        title: "Secret still referenced by model entries",
        hint: "Model registry entries still reference this secret. Remove them first, then delete it. The error detail names the count.",
        user_message: Some(
            "This key is used by one or more models in your registry. Remove those entries first, then delete the key.",
        ),
    },
    Problem {
        code: error_code::SECRET_NAME_TAKEN,
        status: 409,
        title: "Secret name already taken",
        hint: "A secret with that name already exists in this workspace. Create never overwrites: update the existing secret, or delete it first.",
        user_message: Some(
            "A secret with that name already exists. Rename this one, or open the existing secret and replace its value.",
        ),
    },
    // The webhook family carries no dashboard sentence, and that is a fact
    // about who reads it rather than an omission: every one of these is
    // rendered in a PROVIDER's delivery log — GitHub's, Slack's, Svix's — to an
    // operator debugging an integration, and never in this product's console.
    // `error_entries.zig` marks every one of them `reachable: no` for the same
    // reason, and `test_entries_match_the_zig_registry` pins the absence both
    // ways. The last three are narrower still: they are never rendered as a
    // problem at all, because the App ingress names them as the REASON in a
    // 200 rather than raising them. They are declared here so that the two
    // registries stay each other's mirror — a code with no entry resolves to
    // UNKNOWN and would answer 500 if one ever were raised.
    Problem {
        code: error_code::WEBHOOK_FLEET_NOT_FOUND,
        status: 404,
        title: "Fleet not found for webhook",
        hint: "No fleet is registered for this webhook endpoint.",
        user_message: None,
    },
    Problem {
        code: error_code::WEBHOOK_MALFORMED,
        status: 400,
        title: "Malformed webhook",
        hint: "Webhook payload could not be parsed. Check Content-Type and body.",
        user_message: None,
    },
    Problem {
        code: error_code::WEBHOOK_SIGNATURE_INVALID,
        status: 401,
        title: "Invalid webhook signature",
        hint: "The webhook signature did not verify. The stored signing secret must match the one configured upstream.",
        user_message: None,
    },
    Problem {
        code: error_code::WEBHOOK_TIMESTAMP_STALE,
        status: 401,
        title: "Stale webhook timestamp",
        hint: "The webhook timestamp is more than 5 minutes off. That means a replay or clock skew.",
        user_message: None,
    },
    Problem {
        code: error_code::WEBHOOK_CREDENTIAL_NOT_CONFIGURED,
        status: 401,
        title: "Webhook credential not configured",
        hint: "The trigger's source is not a recognized webhook provider (github, slack, linear), or the workspace holds no secret for it. Set a recognized source. Then store a random value of at least 32 bytes in that secret's webhook_secret field, set the same upstream, and resend.",
        user_message: None,
    },
    Problem {
        code: error_code::WEBHOOK_INSTALL_NOT_MAPPED,
        status: 404,
        title: "Connector installation is not mapped",
        hint: "Reconnect the provider App to the intended workspace, then redeliver the event.",
        user_message: None,
    },
    Problem {
        code: error_code::WEBHOOK_SUBSCRIPTION_NOT_FOUND,
        status: 404,
        title: "No fleet subscription matched",
        hint: "Bind the repository and event to an active fleet with an approved integration grant.",
        user_message: None,
    },
    Problem {
        code: error_code::WEBHOOK_PAYLOAD_TOO_LARGE,
        status: 413,
        title: "Webhook payload too large",
        hint: "The webhook body exceeds the 1 MiB limit. Reduce the payload size.",
        user_message: None,
    },
    Problem {
        code: error_code::SCHEDULE_NOT_FOUND,
        status: 404,
        title: "Schedule not found",
        hint: "No schedule matches this identifier for the selected Fleet.",
        user_message: None,
    },
    Problem {
        // 409, not 400: the Zig answers `conflict` and a status is a property
        // of the CODE — the two binaries disagreeing about it is what sends a
        // client down a branch the other daemon never produces.
        code: error_code::SCHEDULE_LIMIT_REACHED,
        status: 409,
        title: "Schedule limit reached",
        hint: "A Fleet can hold at most 32 schedules. Remove one before creating another.",
        user_message: None,
    },
    Problem {
        code: error_code::SCHEDULE_KEY_TAKEN,
        status: 409,
        title: "Schedule source already exists",
        hint: "A schedule already uses this source key for the selected Fleet. Pick a different source or update the existing schedule.",
        user_message: None,
    },
    Problem {
        code: error_code::SCHEDULE_SYNCING,
        status: 409,
        title: "Schedule update busy",
        hint: "Another schedule mutation holds the synchronization lease. Retry after the reported delay.",
        user_message: None,
    },
    Problem {
        code: error_code::APPROVAL_NOT_FOUND,
        status: 404,
        title: "Approval not found",
        hint: "No gate under that id in this workspace. A gate id from another workspace reads as absent on purpose \u{2014} the scope is an authorization, not a filter.",
        user_message: Some(
            "That approval action wasn't found. It may have already timed out or been resolved elsewhere.",
        ),
    },
    Problem {
        code: error_code::APPROVAL_ALREADY_RESOLVED,
        status: 409,
        title: "Approval already resolved",
        hint: "Already resolved by Slack, the dashboard, or a timeout. The response body carries the outcome and resolver.",
        user_message: Some(
            "Someone already resolved this. Refresh to see the outcome and who resolved it.",
        ),
    },
    Problem {
        code: error_code::PREF_KEY_UNKNOWN,
        status: 400,
        title: "Unknown preference key",
        hint: "The path names a key outside the writable preference registry. Only the keys the dashboard declares can be written; anything else is refused here rather than stored.",
        user_message: Some("That setting doesn't exist. Reload the page and try again."),
    },
    Problem {
        code: error_code::PREF_VALUE_TOO_LARGE,
        status: 400,
        title: "Preference value too large",
        hint: "A preference value exceeds 1 KiB. A preference holds one small toggle, not a document; store larger state where it belongs.",
        user_message: Some("That setting is too large to save. Reload the page and try again."),
    },
    Problem {
        code: error_code::WORKSPACE_NAME_EXISTS,
        status: 409,
        title: "Workspace name already exists",
        hint: "A workspace in this tenant already uses that name. Check the refreshed list or choose another name.",
        user_message: Some(
            "A workspace with that name already exists. Check the refreshed list or choose another name.",
        ),
    },
    Problem {
        code: error_code::LIBRARY_CURSOR_MALFORMED,
        status: 400,
        title: "Pagination cursor is malformed",
        hint: "`starting_after` is not a cursor this endpoint issued. Send back the previous page's `next_cursor` verbatim; never compose one.",
        user_message: Some(
            "That page link is no longer valid. Go back to the first page and try again.",
        ),
    },
    Problem {
        code: error_code::LIBRARY_CURSOR_MISMATCH,
        status: 400,
        title: "Pagination cursor does not match this request",
        hint: "The cursor belongs to a different query: its tenant, workspace, filters, or limit differ. After changing filters, start from the first page.",
        user_message: Some(
            "The filters changed since that page was loaded. Start again from the first page.",
        ),
    },
    Problem {
        code: error_code::LIBRARY_INPUT_OUT_OF_BOUNDS,
        status: 400,
        title: "Pagination or filter input out of bounds",
        hint: "`limit` must be 1 to 100, and a filter value at most 128 bytes. Use a smaller page or a shorter value.",
        user_message: Some(
            "That request asked for too much at once. Try a smaller page size or a shorter filter.",
        ),
    },
    Problem {
        code: error_code::LIBRARY_DB_UNAVAILABLE,
        status: 503,
        title: "Data service temporarily unavailable",
        hint: "The database query failed transiently. Retrying is safe.",
        user_message: Some("We couldn't reach the database. Try again shortly."),
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
        code: error_code::STARTUP_ENV_CHECK,
        status: 500,
        title: "Required settings missing",
        hint: "Required environment variables are missing. Run 'agentsfleetd doctor' to see which ones.",
        // Pre-listen: this is reported to the product analytics and to stderr,
        // never as an HTTP response, so there is no dashboard to render a
        // sentence into. The Zig entry carries the same reachability note.
        user_message: None,
    },
    Problem {
        code: error_code::STARTUP_DB_CONNECT,
        status: 500,
        title: "Data service unavailable",
        hint: "Database is unreachable. Check that DATABASE_URL is set and the database accepts connections.",
        user_message: None,
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
];
