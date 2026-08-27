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
        code: error_code::VAULT_DATA_INVALID,
        status: 400,
        title: "Secret data must be a non-empty JSON object",
        hint: "The body's 'data' field must be a JSON object with at least one key. Strings, arrays, scalars, and `{}` are rejected.",
        user_message: Some(
            "That secret needs at least one field. Enter it as a JSON object with one or more keys — not a bare string or list.",
        ),
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
