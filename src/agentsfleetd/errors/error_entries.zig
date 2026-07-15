/// error_entries.zig — single source of truth for all error code entries.
///
/// Each Entry has 5 required fields: code, http_status, title, hint, docs_uri,
/// plus an optional `user_message` (null unless authored via `eu()`).
/// The ENTRIES array is imported by error_registry.zig which builds the
/// comptime lookup map and re-exports ERR_* constants.
///
/// To add a new error code: add one e() call to ENTRIES, then add the
/// corresponding ERR_* constant in error_registry.zig. If the code is
/// reachable from a dashboard surface, use eu() instead and author a
/// `user_message`: `hint` is written for the CLI/API-integrator audience
/// (precise, technical) and is often jargon for a dashboard end user;
/// `user_message` is the one-sentence, dashboard-safe alternative,
/// authored once here rather than duplicated in the frontend's CODE_MAP.
///
/// Authoring rule (the project-local home for this rule; `docs/LOGGING_STANDARD.md`
/// and `audits/error-codes.sh` are both dotfiles symlinks with cross-project
/// blast radius, so project-specific registry rules live here instead):
///   1. Distinct failure => distinct code. A caller-visible
///      internalOperationError() detail must read as plain English, never
///      jargon (component/schema names, alloc/OOM, state-machine phrasing, a
///      raw @errorName tag) — see internal_op_error_sweep_test.zig, enforced
///      standing via a justify-per-add `// mudball-ok: <reason>` requirement.
///   2. Reachable => user_message. Every e()-only entry carries a
///      `// reachable: no — <reason>` annotation; `reachable: yes` without a
///      user_message (eu()) fails error_entries_reachability_test.zig.
///   3. error-codes.mdx is generated (`make gen-error-codes`), never
///      hand-synced — drift between the registry and that page is
///      structurally impossible.
const std = @import("std");

pub const ERROR_DOCS_BASE = "https://docs.agentsfleet.net/api-reference/error-codes#";

const S_UZ_INTERNAL_003 = "UZ-INTERNAL-003";
const S_TITLE_REQUEST_FAILED = "Request failed";
const S_TITLE_SECRET_NOT_FOUND = "Secret not found";

pub const Entry = struct {
    code: []const u8,
    http_status: std.http.Status,
    title: []const u8,
    hint: []const u8,
    docs_uri: []const u8,
    /// Dashboard-safe one-sentence alternative to `hint`/`title`. Null for
    /// every code not curated via eu() — the RFC 7807 body omits the field
    /// entirely rather than serializing `user_message: null`.
    user_message: ?[]const u8 = null,
};

/// Sentinel for unrecognized codes. Defined OUTSIDE ENTRIES — collision
/// is structurally impossible (enforced by comptime assertion in error_registry.zig).
pub const UNKNOWN = Entry{
    .code = "UZ-UNKNOWN",
    .http_status = .internal_server_error,
    .title = "Unknown error",
    .hint = "This error code is not registered. Report to the operator.",
    .docs_uri = ERROR_DOCS_BASE ++ S_UZ_INTERNAL_003,
};

fn e(
    comptime code: []const u8,
    comptime status: std.http.Status,
    comptime title: []const u8,
    comptime hint_text: []const u8,
) Entry {
    return .{
        .code = code,
        .http_status = status,
        .title = title,
        .hint = hint_text,
        .docs_uri = ERROR_DOCS_BASE ++ code,
    };
}

/// Like e(), plus a curated `user_message` — see the module doc for when to
/// reach for this instead of e().
fn eu(
    comptime code: []const u8,
    comptime status: std.http.Status,
    comptime title: []const u8,
    comptime hint_text: []const u8,
    comptime user_message_text: []const u8,
) Entry {
    var entry = e(code, status, title, hint_text);
    entry.user_message = user_message_text;
    return entry;
}

pub const ENTRIES = [_]Entry{
    // ── UUIDV7 ──────────────────────────────────────────────────────────────
    e("UZ-UUIDV7-009", .bad_request, "Invalid identifier shape", "The supplied identifier does not match the expected version 7 universally unique identifier shape."), // reachable: no — CLI/API path-param validator, never a dashboard-fetch path
    // ── INTERNAL ─────────────────────────────────────────────────────────────
    eu("UZ-INTERNAL-001", .service_unavailable, "Service unavailable", "Check that DATABASE_URL is set and the database server is reachable. Run 'agentsfleetd doctor' to verify.", "A required service is unavailable. Try again shortly."),
    eu("UZ-INTERNAL-002", .internal_server_error, S_TITLE_REQUEST_FAILED, "A database query failed. Check the err= field and database logs.", "We couldn't finish that request. Try again shortly."),
    e(S_UZ_INTERNAL_003, .internal_server_error, S_TITLE_REQUEST_FAILED, "An internal operation failed. Check the err= field for details. " ++
        "If the error continues, check service connectivity and run 'agentsfleetd doctor'."), // reachable: no — deliberately generic catch-all; a specific dashboard-reachable failure gets promoted out of this bucket instead
    // ── REQUEST ──────────────────────────────────────────────────────────────
    eu("UZ-REQ-001", .bad_request, "Invalid request", "The request body or parameters are invalid. Check the API documentation.", "That request wasn't valid. Double-check the values you entered and try again."),
    e("UZ-REQ-002", .payload_too_large, "Payload too large", "Request body exceeds the maximum allowed size."), // reachable: no — CLI/API request-size guard, never a dashboard-fetch path
    // ── AUTH ─────────────────────────────────────────────────────────────────
    eu("UZ-AUTH-001", .forbidden, "Forbidden", "Access denied. Check that your API key has the required role.", "You need operator access for that. Ask a tenant operator or admin to manage API keys."),
    e("UZ-AUTH-002", .unauthorized, "Unauthorized", "Authentication required. Provide a valid Bearer token."), // reachable: no — CLI/API-key surface, not fetched by ui/packages/app
    e("UZ-AUTH-003", .unauthorized, "Token expired", "Your authentication token has expired. Re-authenticate."), // reachable: no — CLI/API-key surface, not fetched by ui/packages/app
    e("UZ-AUTH-004", .service_unavailable, "Authentication service unavailable", "Authentication service is temporarily unavailable. Retry shortly."), // reachable: no — CLI/API-key surface, not fetched by ui/packages/app
    e("UZ-AUTH-005", .not_found, "Session not found", "Session was not found. It may have expired or been invalidated."), // reachable: no — CLI/API-key surface, not fetched by ui/packages/app
    e("UZ-AUTH-006", .unauthorized, "Session expired", "Your session has expired. Please sign in again."), // reachable: no — CLI/API-key surface, not fetched by ui/packages/app
    e("UZ-AUTH-011", .bad_request, "Verification code did not match", "The 6-digit verification code did not match what the dashboard issued. " ++
        "Double-check the code shown in your browser and try again."), // reachable: no — `agentsfleet login` CLI device-flow, rendered in the terminal not the dashboard
    e("UZ-AUTH-012", .gone, "Login session already consumed", "This login session has already been consumed. Start over with `agentsfleet login`."), // reachable: no — CLI login flow
    e("UZ-AUTH-013", .gone, "Login session aborted", "This login session was aborted (too many wrong codes, explicit cancel, or replaced by a newer session). " ++
        "Start over with `agentsfleet login`."), // reachable: no — CLI login flow
    e("UZ-AUTH-014", .conflict, "Login session not approved", "This login session has not been approved in the dashboard yet. " ++
        "Approve it in your browser before submitting a verification code."), // reachable: no — CLI login flow (the dashboard side is an approve action, not a fetch of this code)
    e("UZ-AUTH-015", .conflict, "Login session already approved", "This login session has already been approved. Do not call /approve a second time."), // reachable: no — CLI login flow
    e("UZ-AUTH-016", .bad_request, "Invalid command-line public key", "The supplied public_key is malformed. Expect base64url-encoded P-256 SubjectPublicKeyInfo."), // reachable: no — CLI login flow
    e("UZ-AUTH-017", .bad_request, "Invalid token name", "token_name must contain 1 to 64 characters from space through tilde."), // reachable: no — CLI login flow
    e("UZ-AUTH-018", .bad_request, "Invalid verification code shape", "verification_code must contain exactly 6 decimal digits."), // reachable: no — CLI login flow
    e("UZ-AUTH-019", .bad_request, "Invalid ciphertext", "ciphertext is missing or empty. Expect a base64url-encoded AES-256-GCM output."), // reachable: no — CLI login flow
    e("UZ-AUTH-020", .bad_request, "Invalid nonce", "nonce is missing, empty, or the wrong length. Expect a base64url-encoded 12-byte value."), // reachable: no — CLI login flow
    eu("UZ-AUTH-022", .forbidden, "Insufficient scope", "Your token does not carry a scope required for this action. The required scope is named in the error detail; see the [Scopes](/api-reference/scopes) reference for what each one grants.", "You need an additional scope for that. Ask an agentsfleet admin to grant the scope this action requires."),
    // ── API (serving-plane backpressure) ─────────────────────────────────────
    e("UZ-API-001", .too_many_requests, "Too many requests", "The API is handling its maximum number of requests. Wait for the Retry-After delay, then retry."), // reachable: no — instance-wide backpressure shed, hit before routing; not a rendered dashboard error
    e("UZ-API-002", .service_unavailable, "Activity stream capacity reached", "The API is serving its maximum number of activity streams. Close unused dashboard tabs or retry shortly."), // reachable: no — SSE connect rejection surfaces as a stream-level reconnect, not a rendered problem+json page

    // ── WORKSPACE ────────────────────────────────────────────────────────────
    // ── BILLING ──────────────────────────────────────────────────────────────
    // ── AGENT ────────────────────────────────────────────────────────────────
    e("UZ-FLEETKEY-001", .not_found, "Fleet key not found", "Fleet key not found. Verify the fleet_key_id."), // reachable: no — CLI/API-key surface, not fetched by ui/packages/app
    // ── WEBHOOK ──────────────────────────────────────────────────────────────
    e("UZ-WH-001", .not_found, "Fleet not found for webhook", "No fleet is registered for this webhook endpoint."), // reachable: no — external webhook sender sees this, not the dashboard
    e("UZ-WH-002", .bad_request, "Malformed webhook", "Webhook payload could not be parsed. Check Content-Type and body."), // reachable: no — external webhook sender sees this, not the dashboard
    // UZ-WH-003 retired (paused-ingress rework): a webhook to a paused fleet answers
    // 200 {"ignored":"fleet_paused"} — sender retry queues add no value for
    // an intentionally paused fleet. Steer ingress refuses with UZ-AGT-012.
    e("UZ-WH-010", .unauthorized, "Invalid webhook signature", "Webhook signature verification failed. Confirm the signing secret " ++
        "stored for this provider (Slack/Clerk/other) matches the one configured " ++
        "upstream."), // reachable: no — external webhook sender sees this, not the dashboard
    e("UZ-WH-011", .unauthorized, "Stale webhook timestamp", "Webhook request timestamp is outside the allowed 5-minute drift window. " ++
        "This may indicate a replay attack or clock skew."), // reachable: no — external webhook sender sees this, not the dashboard
    e("UZ-WH-020", .unauthorized, "Webhook credential not configured", "Create a random webhook secret with at least 32 bytes. " ++
        "Store it as this source's webhook_secret, configure the same value upstream, then resend."), // reachable: no — external webhook sender sees this, not the dashboard
    e("UZ-WH-021", .not_found, "Connector installation is not mapped", "Reconnect the provider App to the intended workspace before redelivering the event."), // reachable: no — log-only ingress outcome
    e("UZ-WH-022", .not_found, "No fleet subscription matched", "Bind the repository and event to an active fleet with an approved integration grant."), // reachable: no — log-only ingress outcome
    e("UZ-WH-030", .payload_too_large, "Webhook payload too large", "Webhook body exceeds the 1 MiB ingest limit. Reduce the payload size " ++
        "or filter at the source."), // reachable: no — external webhook sender sees this, not the dashboard
    // ── SLACK CONNECTOR ──────────────────────────────────────────────────────
    // Events ingress (M106 §2). 010/011 are the only rejections Slack ever sees
    // as a 4xx; 020 is a benign 200-ack no-op (an uninstalled team), so its
    // entry carries `.ok` — the code is a structured telemetry/log reason, never
    // an `hx.fail` wire status (the handler returns 200 via `hx.ok`).
    e("UZ-SLK-010", .unauthorized, "Invalid Slack signature", "The Slack request signature did not verify. Confirm the platform Slack " ++
        "app signing secret matches the one vaulted at slack-app/signing_secret."), // reachable: no — Slack-to-server events ingress, not a dashboard fetch
    e("UZ-SLK-011", .unauthorized, "Stale Slack timestamp", "The Slack request timestamp is outside the allowed 5-minute drift window — " ++
        "a replay attempt or a skewed server clock."), // reachable: no — Slack-to-server events ingress, not a dashboard fetch
    e("UZ-SLK-020", .ok, "Slack team not installed", "The Slack team that sent this event has no connector install, so the event " ++
        "is acknowledged (200) and ignored. Re-run Connect Slack in the dashboard to (re)install."), // reachable: no — a 200-ack log/telemetry reason, never returned as an hx.fail wire status
    e("UZ-SLK-022", .bad_gateway, "Slack token exchange failed", "The Slack OAuth code could not be exchanged for a bot token. Retry the " ++
        "connect flow; if the error continues, verify the platform Slack app credentials."), // reachable: no — server-side OAuth exchange step, not a page the dashboard fetches and renders
    // 030 is log-only: the connector:outbound worker posts the answer async, so a
    // failed chat.postMessage is logged + retried with backoff, never surfaced to
    // an HTTP caller. The status is nominal (mirrors 022 — a Slack-upstream fault).
    e("UZ-SLK-030", .bad_gateway, "Slack answer post failed", "The channel bot's answer could not be delivered to Slack (missing chat:write, " ++
        "a 429, or a Slack outage). It is logged and retried with backoff; the run itself never fails."), // reachable: no — async worker log-only, never returned to an HTTP caller
    // ── TOOL ─────────────────────────────────────────────────────────────────
    e("UZ-TOOL-005", .bad_request, "Unknown tool", "Unknown tool name. Check spelling against the known tools list."), // reachable: no — surfaces in the activity stream via NullClaw, not a fetched dashboard page (see error-codes.mdx note)
    // ── AGENT ───────────────────────────────────────────────────────────────
    e("UZ-AGT-003", .failed_dependency, "Fleet credential missing", "A required credential is not in the vault. Add it with: `agentsfleet secret create <NAME>`"), // reachable: no — CLI/API-key surface, not fetched by ui/packages/app
    e("UZ-AGT-004", .internal_server_error, "Fleet unavailable", "Fleet could not be claimed from the database. Check that the fleet_id exists and status is 'active'."), // reachable: no — CLI/API-key surface, not fetched by ui/packages/app
    e("UZ-AGT-006", .conflict, "Fleet name already exists", "A Fleet with this name already exists. Use `agentsfleet kill <name>` first, then deploy again."), // reachable: no — CLI/API-key surface, not fetched by ui/packages/app
    // UZ-AGT-007 retired (single-string credential body) → see UZ-VAULT-002.
    eu("UZ-AGT-008", .bad_request, "Invalid fleet config", "Config JSON is malformed. Verify trigger, tools, credentials, and budget fields " ++
        "in your TRIGGER.md frontmatter. See the [Authoring a fleet](/fleets/authoring) guide for a working example.", "That fleet's config isn't valid. Check the trigger, tools, credentials, and budget fields, then try again."),
    eu("UZ-AGT-009", .not_found, "Fleet not found", "Fleet not found. Verify the fleet_id and that it has not been killed.", "We couldn't find that Fleet. It may have been deleted, or the identifier doesn't match one in this workspace."),
    eu("UZ-AGT-010", .conflict, "Fleet state transition not allowed", "The requested lifecycle action is not valid from the fleet's current state. The response detail names the specific transition that was refused.", "That action isn't available for this Fleet right now — check its current status and try again."),
    eu("UZ-AGT-011", .bad_request, "Fleet files disagree on `name:`", "Top-level `name:` in `SKILL.md` must match `name:` in `TRIGGER.md`. Use one identity per Fleet Bundle.", "This Fleet Bundle's files disagree on its name. `SKILL.md` and `TRIGGER.md` must match. Fix the source and try again."),
    eu("UZ-AGT-012", .conflict, "Fleet is paused", "This fleet is not active and refuses new work. Resume it with: `agentsfleet resume <fleet>`, then retry.", "This Fleet is paused. Resume it before sending new work."),
    eu("UZ-AGT-013", .internal_server_error, "Fleet install rolled back", "Event-stream setup failed during create; the fleet row was rolled back so the caller can retry cleanly. If the error continues, check queue connectivity.", "We couldn't finish setting up your fleet. Nothing was created — try again."),
    eu("UZ-AGT-014", .precondition_failed, "Fleet source is stale", "The `If-Match` header names a source version the fleet has moved past — someone else saved first. The response body carries the current `etag`: re-read the fleet, rebase your edit on it, and retry.", "Someone else edited this Fleet's source since you opened it. Reload to see their change, then re-apply your edit."),
    // ── Fleet Bundle ───────────────────────────────────────────────────────
    eu("UZ-BUNDLE-001", .bad_request, "Invalid Fleet Bundle", "The supplied Fleet Bundle is missing `SKILL.md` or contains unsafe, oversized, or malformed files.", "That Fleet Bundle isn't valid. It's missing `SKILL.md`, or has an unsafe or oversized file. Check the source and try again."),
    eu("UZ-BUNDLE-002", .not_found, "Fleet Bundle not found", "No installable library entry or stored snapshot matches the request in this workspace.", "We couldn't find that Fleet Bundle. It may not be installed in this workspace yet — check the Fleet library."),
    eu("UZ-BUNDLE-003", .failed_dependency, "Fleet Bundle secrets missing", "Add the missing workspace secrets before installing this Fleet Bundle.", "This Fleet Bundle needs secrets this workspace doesn't have yet. Add the missing secrets, then install again."),
    eu("UZ-BUNDLE-004", .bad_gateway, "Fleet Bundle fetch failed", "The Fleet Bundle source could not be fetched from GitHub. The repository may be missing or private, or GitHub may be unreachable. Verify the source reference and retry.", "We couldn't fetch that Fleet Bundle from GitHub. Check the source and try again."),
    eu("UZ-BUNDLE-005", .service_unavailable, "Fleet Bundle storage unavailable", "Snapshot storage is not configured or is unavailable, so the validated bundle could not be stored. Retry later or contact the operator.", "We couldn't store your Fleet Bundle right now. Try again shortly."),
    // UZ-BUNDLE-006 retired: no producer ever emitted it.
    // ── VAULT ────────────────────────────────────────────────────────────────
    eu("UZ-VAULT-001", .bad_request, "Secret data must be a non-empty JSON object", "POST /secrets body must include a 'data' field that is a JSON object with at least one key. " ++
        "Bare strings, arrays, scalars, and `{}` are rejected.", "That secret needs at least one field. Enter it as a JSON object with one or more keys — not a bare string or list."),
    eu("UZ-VAULT-002", .bad_request, "Secret data too large", "Stringified secret data exceeds 4 KiB. Compose the secret from fewer or shorter fields.", "That secret is too large. Keep it under 4 KiB. Trim or shorten the fields."),
    eu("UZ-VAULT-003", .not_found, S_TITLE_SECRET_NOT_FOUND, "No secret matches this name in the workspace. List the workspace secrets to find a valid name, or create it first.", "We couldn't find that secret. It may have already been deleted — refresh the list."),
    eu("UZ-VAULT-004", .conflict, "Secret still referenced by model entries", "One or more core.tenant_model_entries rows reference this secret_ref (M121). Remove those entries first, then delete the secret. The response detail names the exact count.", "This key is used by one or more models in your registry. Remove those entries first, then delete the key."),
    // ── PROVIDER (PUT /v1/tenants/me/provider) ───────────────────────────────
    eu("UZ-PROVIDER-001", .bad_request, "secret_ref required when mode=self_managed", "PUT body must include `secret_ref` naming a vault credential when `mode` is self_managed.", "Pick a secret to activate. Choose a stored secret before switching to a self-managed model."),
    eu("UZ-PROVIDER-002", .bad_request, S_TITLE_SECRET_NOT_FOUND, "The named secret_ref has no vault row in the tenant's primary workspace. " ++
        "Run `agentsfleet secret create <NAME> --data=@-` to create it.", "We couldn't find that secret. Store it under Secrets & ENVs, then try again."),
    eu("UZ-PROVIDER-003", .bad_request, "Secret JSON missing required field", "Stored secret JSON must include `provider` (a non-empty string); `api_key` is required for a named provider but optional for an `openai-compatible` endpoint. `model` is optional — the model registry entry carries it, not the credential. " ++
        "Run `agentsfleet secret create` again with the required fields.", "That secret is missing required fields. It needs a provider set (and an API key for a named provider) — edit it under Secrets & ENVs and add them."),
    eu("UZ-PROVIDER-004", .bad_request, "Model not in library", "The effective model is not present in core.model_library. Pick a model from the model library (GET /v1/models, or the admin list) " ++
        "or request the library be extended.", "That model isn't in our library yet. Pick a listed model, or ask us to add support for it."),
    eu("UZ-PROVIDER-005", .bad_request, "Custom endpoint base_url invalid or unsafe", "An openai-compatible credential needs a valid `base_url`: it must use https and must not target a " ++
        "loopback, private, link-local, or cloud-metadata host. A non-openai-compatible provider must not carry a `base_url`.", "That endpoint URL isn't allowed. Use a public https URL for your custom endpoint."),
    eu("UZ-PROVIDER-006", .not_found, "Library model not found", "No core.model_library row matches this id. List the library to find a valid id, or add the model first.", "We couldn't find that model in the library. Refresh the list and try again."),
    eu("UZ-PROVIDER-007", .conflict, "Library model is the active platform default", "This model is the active platform default. Point the default at another library model before deleting it.", "This model is the active platform default — point the default at another model before deleting it."),
    eu("UZ-PROVIDER-008", .conflict, "Library model already exists", "A library row for this provider and model already exists. Edit the existing row instead of adding a duplicate.", "That model is already in the library. Edit the existing entry instead of adding a duplicate."),
    eu("UZ-PROVIDER-009", .internal_server_error, "Platform model key not configured", "No active row in core.platform_provider_defaults. An operator must set one via PUT /admin/platform-keys before tenants can switch to platform defaults.", "Platform defaults aren't set up on this deployment yet. Keep your current provider for now, or contact support."),
    eu("UZ-PROVIDER-010", .internal_server_error, "Tenant has no primary workspace", "The tenant row has no primary workspace — an onboarding invariant that should always hold. Contact support with the request id.", "Something's off with your account setup. Contact support with the request id below."),
    // ── MODELS (tenant model registry, /v1/tenants/me/models — M121) ─────────
    eu("UZ-MODELS-001", .conflict, "Cannot delete the active model entry", "This entry is the tenant's current active selection. Switch to a different entry first, then delete this one.", "This is your active model — switch to a different one first, then remove this entry."),
    eu("UZ-MODELS-002", .not_found, "Referenced secret not found", "POST/PATCH secret_ref does not name a vault secret in the tenant's primary workspace. Store the secret first, or pick an existing one.", "We couldn't find that key. Store it under Secrets & ENVs first, or pick an existing key."),
    eu("UZ-MODELS-003", .conflict, "Model entry already exists", "An entry with this exact (model_id, secret_ref) pair already exists for this tenant. Edit the existing entry instead of adding a duplicate.", "You already have this model registered with that key. Edit the existing entry instead."),
    eu("UZ-MODELS-004", .not_found, "Model entry not found", "No core.tenant_model_entries row matches this id for the calling tenant. It may have already been deleted — refresh the list.", "We couldn't find that model entry. It may have already been removed — refresh the list."),
    // ── CATALOG (platform fleet-library lifecycle, /v1/admin/fleet-libraries — M128) ──
    eu("UZ-CATALOG-001", .not_found, "Fleet library entry not found", "No core.fleet_library row matches this catalog id. It may have already been deleted — refresh the catalog.", "We couldn't find that fleet. It may have already been removed — refresh the page."),
    eu("UZ-CATALOG-002", .conflict, "Cannot publish a fleet with no bundle", "This entry has no content_hash: no bundle has ever been fetched for it, so there is nothing to serve a tenant. Fetch the bundle from its repository first, then publish.", "There's no bundle for this fleet yet. Fetch it from its repository first, then publish."),
    eu("UZ-CATALOG-003", .conflict, "Cannot delete a published fleet", "This entry is live in every workspace's gallery and installable. Unpublish it first, then delete — that way no tenant loses a fleet mid-install.", "This fleet is published. Unpublish it first, then delete it."),
    eu("UZ-CATALOG-004", .conflict, "Catalog id already taken by another repository", "The bundle's SKILL.md frontmatter name resolves to a catalog id that already exists under a DIFFERENT source repository. Replacing it would swap the content every tenant installs. Rename the bundle, or retry with replace to overwrite deliberately.", "A different repository already owns this fleet's name. Rename the bundle, or confirm you want to replace it."),
    eu("UZ-CATALOG-005", .precondition_failed, "Catalog entry changed since you loaded it", "The `If-Match` header names a version the catalog row has moved past — another operator saved first. The response body carries the current `etag`. Refetch the row, rebase your edit, and retry — a blind re-send here could repoint the source and unpublish the fleet.", "Someone else edited this catalog entry since you opened it. Refresh to see their change, then re-apply your edit."),
    // ── GATE ─────────────────────────────────────────────────────────────────
    // ── STARTUP ──────────────────────────────────────────────────────────────
    e("UZ-STARTUP-001", .internal_server_error, "Required settings missing", "Required environment variables are missing. Run 'agentsfleetd doctor' to see which ones."), // reachable: no — pre-listen boot check, never an HTTP response
    e("UZ-STARTUP-002", .internal_server_error, "Settings could not load", "Configuration failed to load. Check that all required env vars are set. " ++
        "Run 'agentsfleetd doctor' to verify."), // reachable: no — pre-listen boot check, never an HTTP response
    e("UZ-STARTUP-003", .internal_server_error, "Data service unavailable", "Database is unreachable. Check that DATABASE_URL is set and the database accepts connections."), // reachable: no — pre-listen boot check, never an HTTP response
    e("UZ-STARTUP-004", .internal_server_error, "Event service unavailable", "Redis is unreachable. Check that REDIS_URL_API is set " ++
        "and the Redis server accepts connections. Run 'agentsfleetd doctor' to verify."), // reachable: no — pre-listen boot check, never an HTTP response
    e("UZ-STARTUP-005", .internal_server_error, "Stored data is not ready", "Database migration state could not be verified. Check database connectivity."), // reachable: no — pre-listen boot check, never an HTTP response
    e("UZ-STARTUP-006", .internal_server_error, "Service could not start", "An environment variable could not be allocated at startup (out of memory). " ++
        "A required secret fails the boot closed; optional config falls back to its default — check host memory pressure."), // reachable: no — pre-listen boot check, never an HTTP response
    // ── RUNNER (agentsfleet-runner /v1/runners control contract) ───────────────────
    e("UZ-RUN-001", .unauthorized, "Invalid runner token", "The Bearer runner_token is missing, malformed, or not recognized. Re-register the runner."), // reachable: no — runner-daemon-to-control-plane wire contract, not dashboard-facing
    e("UZ-RUN-005", .conflict, "Stale fencing token", "The lease was reclaimed by a newer holder. This report is rejected; the current holder's result wins."), // reachable: no — runner-daemon-to-control-plane wire contract, not dashboard-facing
    e("UZ-RUN-006", .not_found, "Lease not found", "No active lease matches this lease_id for the presenting runner; it may have expired, been reclaimed, or never existed."), // reachable: no — runner-daemon-to-control-plane wire contract, not dashboard-facing
    // UZ-RUN-007 retired: no agentsfleetd producer ever emitted it —
    // the agentsfleet-runner binary's separate client_errors.zig mirror uses the
    // same string only as a local log annotation, never as a reported FailureClass.
    e("UZ-RUN-009", .unauthorized, "Runner admin state blocks access", "This runner is cordoned, draining, drained, or revoked and cannot call the runner plane. Re-enroll the host to mint a fresh runner token."), // reachable: no — runner-daemon-to-control-plane wire contract, not dashboard-facing
    e("UZ-RUN-010", .conflict, "Lease exceeded max runtime", "The lease reached its maximum runtime and cannot renew. The runner stops the child and reports any result."), // reachable: no — runner-daemon-to-control-plane wire contract, not dashboard-facing
    e("UZ-RUN-011", .conflict, "Lease lost", "The lease moved to another runner before renewal. The former runner must stop its child."), // reachable: no — runner-daemon-to-control-plane wire contract, not dashboard-facing
    e("UZ-RUN-012", .payment_required, "Lease renewal blocked: no credits", "The tenant balance cannot cover another run slice. The lease does not renew, and the run stops cleanly."), // reachable: no — runner-daemon-to-control-plane wire contract, not dashboard-facing
    e("UZ-RUN-013", .bad_request, "Renew body malformed", "The renew request body could not be parsed. Token counts become zero, but the renewed slice still charges for runtime. The lease still renews."), // reachable: no — runner-daemon-to-control-plane wire contract, not dashboard-facing
    eu("UZ-RUN-014", .not_found, "Runner not found", "No runner matches this runner_id. Verify the platform admin minted the runner before mutating it.", "We couldn't find that runner. It may have been removed — refresh the list."),
    e("UZ-RUN-015", .payment_required, "Lease renewal blocked: fleet budget exhausted", "The fleet reached its daily_dollars or monthly_dollars limit from `TRIGGER.md`. The lease cannot renew, so the run stops. This differs from UZ-RUN-012, which reports a low tenant balance."), // reachable: no — runner-daemon-to-control-plane wire protocol, not dashboard-facing
    // Runtime / execute-path entries (sandbox, runner, relay, credentials,
    // approval-gate, memory, api-keys, grants, tool/credential, proxy,
    // gate-execute) live in error_entries_runtime.zig and are concatenated
    // into REGISTRY by error_registry.zig.
};
