//! Safe caller guidance for private setup, without resource existence details.

use super::Problem;
use crate::error_code;

pub(super) const AUTHORING: &[Problem] = &[
    Problem {
        code: error_code::AUTHORING_REVIEW,
        status: 409,
        title: "Setup review changed",
        hint: "Refetch the effective setup and review the new review_digest before saving.",
        user_message: Some("This setup changed. Review it before saving."),
    },
    Problem {
        code: error_code::AUTHORING_POLICY,
        status: 409,
        title: "Setup policy changed",
        hint: "Review the current effective setup before continuing.",
        user_message: Some("This setup needs a new review."),
    },
    Problem {
        code: error_code::AUTHORING_BALANCE,
        status: 402,
        title: "Authoring balance insufficient",
        hint: "Fund the workspace owner's wallet or wait for outstanding accounting.",
        user_message: Some("The workspace balance cannot cover this authoring request."),
    },
    Problem {
        code: error_code::AUTHORING_TOKEN,
        status: 422,
        title: "Authoring context too large",
        hint: "Reduce context or select an eligible model with a larger window.",
        user_message: Some("This request exceeds the authoring model's context limit."),
    },
    Problem {
        code: error_code::AUTHORING_PROVIDER,
        status: 502,
        title: "Authoring provider failed",
        hint: "The provider returned known usage. Retry explicitly from the draft.",
        user_message: Some("The authoring provider could not finish this request."),
    },
    Problem {
        code: error_code::AUTHORING_UNKNOWN,
        status: 409,
        title: "Authoring usage unresolved",
        hint: "Do not dispatch again. Wait for authoritative provider accounting.",
        user_message: Some("This model request is waiting for accounting."),
    },
    Problem {
        code: error_code::AUTHORING_TIMEOUT,
        status: 504,
        title: "Authoring attempt timed out",
        hint: "Retry only after the provider obligation has been resolved.",
        user_message: Some("This authoring attempt ran out of time."),
    },
    Problem {
        code: error_code::AUTHORING_NOT_FOUND,
        status: 404,
        title: "Setup resource not found",
        hint: "Use a resource available to this person in this workspace.",
        user_message: Some("This setup is unavailable."),
    },
    Problem {
        code: error_code::AUTHORING_STATE,
        status: 409,
        title: "Setup state conflict",
        hint: "Wait for authoring to finish or stop it before changing setup.",
        user_message: Some("Finish or stop the current work before changing setup."),
    },
    Problem {
        code: error_code::AUTHORING_STALE,
        status: 412,
        title: "Setup version changed",
        hint: "Refetch the current setup and retry with its ETag.",
        user_message: Some("This setup changed. Review the latest version."),
    },
    Problem {
        code: error_code::AUTHORING_PRECONDITION,
        status: 428,
        title: "Setup version required",
        hint: "Send the resource ETag in If-Match.",
        user_message: Some("Reload this setup before saving changes."),
    },
    Problem {
        code: error_code::AUTHORING_REPLAY,
        status: 409,
        title: "Operation key conflict",
        hint: "Reuse a key only for the same normalized request and actor.",
        user_message: Some("That operation key already belongs to a different request."),
    },
    Problem {
        code: error_code::AUTHORING_MODEL,
        status: 409,
        title: "Authoring model unavailable",
        hint: "Choose an eligible model configured by the workspace owner.",
        user_message: Some("Choose an available authoring model."),
    },
    Problem {
        code: error_code::AUTHORING_CURSOR,
        status: 400,
        title: "Invalid setup cursor",
        hint: "Restart this list with the same person, filters and page size.",
        user_message: Some("Reload this list to continue."),
    },
    Problem {
        code: error_code::AUTHORING_FENCE,
        status: 409,
        title: "Authoring lease lost",
        hint: "Stop this attempt; refresh the private draft before starting another message.",
        user_message: Some("This authoring attempt can no longer update the draft."),
    },
    Problem {
        code: error_code::AUTHORING_GONE,
        status: 410,
        title: "Setup resource purged",
        hint: "This operation was accepted before its resource was deleted. Start a new operation.",
        user_message: Some("This setup was deleted."),
    },
];
