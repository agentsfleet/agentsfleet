//! What a client is told when a THIRD party is the thing that refused.
//!
//! An integration nobody connected, a broker nobody configured, a GitHub App
//! needing reconnection, an OAuth exchange that failed. None of these resolve by
//! editing anything in this repository, and the sentences say so.

use super::Problem;
use crate::error_code;

/// This family's entries, in `REGISTRY` order.
pub(super) const INTEGRATION: &[Problem] = &[
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
        code: error_code::GRANT_REVOKE_NOT_FOUND,
        status: 404,
        title: "Integration grant not found",
        hint: "No grant with that id exists for this fleet, or it was already revoked. List current grants with `GET /v1/workspaces/{ws}/fleets/{id}/integration-grants`.",
        user_message: Some(
            "We couldn't find that grant request. It may have already been resolved — refresh the list.",
        ),
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
