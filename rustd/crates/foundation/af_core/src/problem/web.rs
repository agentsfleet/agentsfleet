//! Problem responses for web vendor calls.

use super::{Problem, error_code};

pub(super) const WEB: &[Problem] = &[
    Problem {
        code: error_code::WEB_NETWORK_RESTRICTED,
        status: 403,
        title: "Fleet web access is restricted",
        hint: "Enable Allow all on the fleet to use web search and vendor page reads.",
        user_message: None,
    },
    Problem {
        code: error_code::WEB_VENDOR_UNCONFIGURED,
        status: 503,
        title: "Web vendor unavailable",
        hint: "Only the platform operator can restore the web service. Contact support or try again later.",
        user_message: None,
    },
    Problem {
        code: error_code::WEB_VENDORS_COOLING,
        status: 503,
        title: "Web vendors are cooling down",
        hint: "Wait for Retry-After before searching again.",
        user_message: None,
    },
    Problem {
        code: error_code::WEB_VENDORS_FAILED,
        status: 502,
        title: "Web search failed",
        hint: "Each available search vendor failed. Try the search again later.",
        user_message: None,
    },
    Problem {
        code: error_code::WEB_INVALID_URL,
        status: 400,
        title: "Page URL refused",
        hint: "Supply a public HTTPS URL without credentials.",
        user_message: None,
    },
    Problem {
        code: error_code::WEB_RESERVATION_UNAVAILABLE,
        status: 503,
        title: "Web reservation unavailable",
        hint: "Try the tool again when the platform can reserve the request.",
        user_message: None,
    },
    Problem {
        code: error_code::WEB_PAGE_UNAVAILABLE,
        status: 502,
        title: "Page unavailable",
        hint: "Check the page address and its availability.",
        user_message: None,
    },
    Problem {
        code: error_code::WEB_EXTRACT_INVALID_RESPONSE,
        status: 502,
        title: "Invalid page reader response",
        hint: "The reader returned an incomplete or ambiguous response.",
        user_message: None,
    },
    Problem {
        code: error_code::WEB_EXTRACT_UNAVAILABLE,
        status: 502,
        title: "Page reader unavailable",
        hint: "A local read may proceed only when the response authorizes it.",
        user_message: None,
    },
    Problem {
        code: error_code::WEB_DEADLINE_EXCEEDED,
        status: 503,
        title: "Web call deadline expired",
        hint: "The web call ran out of time. Try again later.",
        user_message: None,
    },
];
