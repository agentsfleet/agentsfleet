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
        code: error_code::PROVIDER_SECRET_REF_REQUIRED,
        status: 400,
        title: "secret_ref required when mode=self_managed",
        hint: "PUT body must include `secret_ref` naming a vault credential when `mode` is self_managed.",
        user_message: Some(
            "Pick a secret to activate. Choose a stored secret before switching to a self-managed model.",
        ),
    },
    Problem {
        code: error_code::PROVIDER_SECRET_NOT_FOUND,
        status: 400,
        title: "Secret not found",
        hint: "The named secret_ref does not exist in the tenant's primary workspace. Create it with `agentsfleet secret create <NAME> --data=@-`.",
        user_message: Some(
            "We couldn't find that secret. Store it under Secrets & ENVs, then try again.",
        ),
    },
    Problem {
        code: error_code::PROVIDER_SECRET_DATA_MALFORMED,
        status: 400,
        title: "Secret JSON missing required field",
        hint: "The stored secret must include `provider`. `api_key` is required for a named provider, optional for `openai-compatible`. `model` belongs on the registry entry.",
        user_message: Some(
            "That secret is missing required fields. It needs a provider set (and an API key for a named provider) — edit it under Secrets & ENVs and add them.",
        ),
    },
    Problem {
        code: error_code::PROVIDER_MODEL_NOT_IN_CATALOGUE,
        status: 400,
        title: "Model not in library",
        hint: "That model is not in the model library. Pick one from GET /v1/models, or ask for it to be added.",
        user_message: Some(
            "That model isn't in our library yet. Pick a listed model, or ask us to add support for it.",
        ),
    },
    Problem {
        code: error_code::PROVIDER_BASE_URL_INVALID,
        status: 400,
        title: "Custom endpoint base_url invalid or unsafe",
        hint: "`base_url` must be https and must not target a loopback, private, link-local, or cloud-metadata host. Only an `openai-compatible` credential may carry one.",
        user_message: Some(
            "That endpoint URL isn't allowed. Use a public https URL for your custom endpoint.",
        ),
    },
    Problem {
        code: error_code::PROVIDER_MODEL_NOT_FOUND,
        status: 404,
        title: "Library model not found",
        hint: "No library model matches this id. List the library to find one, or add the model first.",
        user_message: Some(
            "We couldn't find that model in the library. Refresh the list and try again.",
        ),
    },
    Problem {
        code: error_code::PROVIDER_MODEL_IN_USE,
        status: 409,
        title: "Library model is the active platform default",
        hint: "This model is the active platform default. Point the default at another library model before deleting it.",
        user_message: Some(
            "This model is the active platform default — point the default at another model before deleting it.",
        ),
    },
    Problem {
        code: error_code::PROVIDER_MODEL_EXISTS,
        status: 409,
        title: "Library model already exists",
        hint: "A library row for this provider and model already exists. Edit the existing row instead of adding a duplicate.",
        user_message: Some(
            "That model is already in the library. Edit the existing entry instead of adding a duplicate.",
        ),
    },
    Problem {
        code: error_code::PROVIDER_PLATFORM_KEY_MISSING,
        status: 500,
        title: "Platform model key not configured",
        hint: "No platform default model is configured. An operator must set one via PUT /admin/platform-keys first.",
        user_message: Some(
            "Platform defaults aren't set up on this deployment yet. Keep your current provider for now, or contact support.",
        ),
    },
    Problem {
        code: error_code::TENANT_NO_PRIMARY_WORKSPACE,
        status: 500,
        title: "Tenant has no primary workspace",
        hint: "This tenant has no primary workspace, which should never happen. Contact support with the request id.",
        user_message: Some(
            "Something's off with your account setup. Contact support with the request id below.",
        ),
    },
    Problem {
        code: error_code::MODELS_DELETE_ACTIVE,
        status: 409,
        title: "Cannot delete the active model entry",
        hint: "This entry is the tenant's current active selection. Switch to a different entry first, then delete this one.",
        user_message: Some(
            "This is your active model — switch to a different one first, then remove this entry.",
        ),
    },
    Problem {
        code: error_code::MODELS_SECRET_NOT_FOUND,
        status: 404,
        title: "Referenced secret not found",
        hint: "POST/PATCH secret_ref does not name a vault secret in the tenant's primary workspace. Store the secret first, or pick an existing one.",
        user_message: Some(
            "We couldn't find that key. Store it under Secrets & ENVs first, or pick an existing key.",
        ),
    },
    Problem {
        code: error_code::MODELS_DUPLICATE_ENTRY,
        status: 409,
        title: "Model entry already exists",
        hint: "This model and key pair is already registered for this tenant. Edit the existing entry instead.",
        user_message: Some(
            "You already have this model registered with that key. Edit the existing entry instead.",
        ),
    },
    Problem {
        code: error_code::MODELS_ENTRY_NOT_FOUND,
        status: 404,
        title: "Model entry not found",
        hint: "No model entry matches this id for this tenant. It may already be deleted — refresh the list.",
        user_message: Some(
            "We couldn't find that model entry. It may have already been removed — refresh the list.",
        ),
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
        code: error_code::CONNECTOR_NOT_CONFIGURED,
        status: 503,
        title: "Connector not configured",
        hint: "An operator must configure this provider app before workspaces can connect.",
        user_message: Some("This connector isn't set up yet. Contact your operator to enable it."),
    },
    Problem {
        code: error_code::CONNECTOR_STATE_INVALID,
        status: 400,
        title: "Invalid connect state",
        hint: "The connect callback's state was missing, forged, expired, or already used. Start the connect again from the dashboard.",
        user_message: Some(
            "That connection attempt expired or was already used. Start connecting again from the dashboard.",
        ),
    },
    Problem {
        code: error_code::CONNECTOR_VENDOR_DEADLINE,
        status: 502,
        title: "Connector vendor call exceeded its deadline",
        hint: "An outbound provider call timed out or could not reach the provider. Retry once; if it continues, check provider status and network access.",
        user_message: Some("We couldn't reach that service right now. Try again shortly."),
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
        code: error_code::CONNECTOR_UNKNOWN,
        status: 404,
        title: "Unknown connector provider",
        hint: "The `{provider}` segment is not in this deployment's connector registry. Check the dashboard connectors page for the available providers.",
        user_message: Some(
            "We don't recognize that connector. Check the available connectors on the dashboard.",
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
    Problem {
        code: error_code::SSE_STREAM_CAP,
        // 503, not the 429 its neighbour answers, and the difference is real:
        // backpressure means "you are asking too fast" and this means "this
        // instance cannot hold another stream". `public/openapi/paths/fleets.yaml`
        // documents the 503 to clients, so the status is a published contract
        // and not a call this port gets to make.
        status: 503,
        title: "Activity stream capacity reached",
        hint: "The API is at its activity-stream limit. Close unused dashboard tabs or retry shortly.",
        // A refused SSE connect surfaces to a browser as a stream-level
        // reconnect, never as a rendered problem page — the Zig entry carries
        // the same reachability note.
        user_message: None,
    },
];
