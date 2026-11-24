//! Safe caller guidance for a fleet's create, edit and replay conflicts.

use super::Problem;
use crate::error_code;

pub(super) const SETUP: &[Problem] = &[
    Problem {
        code: error_code::SETUP_STATE,
        status: 409,
        title: "Fleet state conflict",
        hint: "Wait for the current work to finish or stop it before changing the fleet.",
        user_message: Some("Finish or stop the current work before changing this fleet."),
    },
    Problem {
        code: error_code::SETUP_STALE,
        status: 412,
        title: "Fleet version changed",
        hint: "Refetch the current resource and retry with its ETag.",
        user_message: Some("This fleet changed. Review the latest version."),
    },
    Problem {
        code: error_code::SETUP_PRECONDITION,
        status: 428,
        title: "Fleet version required",
        hint: "Send the resource ETag in If-Match.",
        user_message: Some("Reload this fleet before saving changes."),
    },
    Problem {
        code: error_code::SETUP_REPLAY,
        status: 409,
        title: "Operation key conflict",
        hint: "Reuse a key only for the same normalized request and actor.",
        user_message: Some("That operation key already belongs to a different request."),
    },
    Problem {
        code: error_code::SETUP_MODEL,
        status: 409,
        title: "Fleet model unavailable",
        hint: "Choose an eligible model configured by the workspace owner.",
        user_message: Some("Choose an available model."),
    },
    Problem {
        code: error_code::SETUP_CURSOR,
        status: 400,
        title: "Invalid list cursor",
        hint: "Restart this list with the same person, filters and page size.",
        user_message: Some("Reload this list to continue."),
    },
    Problem {
        code: error_code::SETUP_GONE,
        status: 410,
        title: "Fleet purged",
        hint: "This operation was accepted before its resource was deleted. Start a new operation.",
        user_message: Some("This fleet was deleted."),
    },
];
