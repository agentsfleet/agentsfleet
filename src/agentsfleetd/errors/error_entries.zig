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
const std = @import("std");

pub const ERROR_DOCS_BASE = "https://docs.agentsfleet.net/api-reference/error-codes#";

const S_UZ_INTERNAL_003 = "UZ-INTERNAL-003";

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
    e("UZ-UUIDV7-009", .bad_request, "Invalid ID shape", "The supplied ID does not match the expected UUIDv7 shape."),
    // ── INTERNAL ─────────────────────────────────────────────────────────────
    eu("UZ-INTERNAL-001", .service_unavailable, "Database unavailable", "Check that DATABASE_URL is set and the database server is reachable. Run 'agentsfleetd doctor' to verify.", "Something broke on our end. Give it another shot — if it keeps failing, send us the code below."),
    eu("UZ-INTERNAL-002", .internal_server_error, "Database error", "A database query failed. Check the err= field and database logs.", "We're under load and dropped your request. Try again in a few seconds."),
    e(S_UZ_INTERNAL_003, .internal_server_error, "Internal operation failed", "An internal operation failed. Check the err= field for details. " ++
        "If persistent, check service connectivity and run 'agentsfleetd doctor'."),
    // ── REQUEST ──────────────────────────────────────────────────────────────
    eu("UZ-REQ-001", .bad_request, "Invalid request", "The request body or parameters are invalid. Check the API documentation.", "That request wasn't valid. Double-check the values you entered and try again."),
    e("UZ-REQ-002", .payload_too_large, "Payload too large", "Request body exceeds the maximum allowed size."),
    // ── AUTH ─────────────────────────────────────────────────────────────────
    eu("UZ-AUTH-001", .forbidden, "Forbidden", "Access denied. Check that your API key has the required role.", "You need operator access for that. Ask a tenant operator or admin to manage API keys."),
    e("UZ-AUTH-002", .unauthorized, "Unauthorized", "Authentication required. Provide a valid Bearer token."),
    e("UZ-AUTH-003", .unauthorized, "Token expired", "Your authentication token has expired. Re-authenticate."),
    e("UZ-AUTH-004", .service_unavailable, "Authentication service unavailable", "Authentication service is temporarily unavailable. Retry shortly."),
    e("UZ-AUTH-005", .not_found, "Session not found", "Session was not found. It may have expired or been invalidated."),
    e("UZ-AUTH-006", .unauthorized, "Session expired", "Your session has expired. Please sign in again."),
    e("UZ-AUTH-011", .bad_request, "Verification code did not match", "The 6-digit verification code did not match what the dashboard issued. " ++
        "Double-check the code shown in your browser and try again."),
    e("UZ-AUTH-012", .gone, "Login session already consumed", "This login session has already been consumed. Start over with `agentsfleet login`."),
    e("UZ-AUTH-013", .gone, "Login session aborted", "This login session was aborted (too many wrong codes, explicit cancel, or replaced by a newer session). " ++
        "Start over with `agentsfleet login`."),
    e("UZ-AUTH-014", .gone, "Login session not approved", "This login session has not been approved in the dashboard yet. " ++
        "Approve it in your browser before submitting a verification code."),
    e("UZ-AUTH-015", .conflict, "Login session already approved", "This login session has already been approved. Do not call /approve a second time."),
    e("UZ-AUTH-016", .bad_request, "Invalid CLI public key", "The supplied public_key is malformed. Expect base64url-encoded P-256 SubjectPublicKeyInfo."),
    e("UZ-AUTH-017", .bad_request, "Invalid token name", "token_name must be 1-64 characters of printable ASCII."),
    e("UZ-AUTH-018", .bad_request, "Invalid verification code shape", "verification_code must be exactly 6 ASCII digits."),
    e("UZ-AUTH-019", .bad_request, "Invalid ciphertext", "ciphertext is missing or empty. Expect a base64url-encoded AES-256-GCM output."),
    e("UZ-AUTH-020", .bad_request, "Invalid nonce", "nonce is missing, empty, or the wrong length. Expect a base64url-encoded 12-byte value."),
    eu("UZ-AUTH-022", .forbidden, "Insufficient scope", "Your token does not carry a scope required for this action. The required scope is named in the error detail; see docs/AUTH.md for the scope catalogue.", "You need an additional scope for that. Ask an agentsfleet admin to grant the scope this action requires."),
    // ── API (serving-plane backpressure) ─────────────────────────────────────
    e("UZ-API-001", .too_many_requests, "Too many in-flight requests", "This API instance is at its in-flight request ceiling and is shedding load. " ++
        "Honor the Retry-After header and retry with backoff. Operators: raise API_MAX_IN_FLIGHT_REQUESTS or add replicas."),
    e("UZ-API-002", .service_unavailable, "Event-stream capacity reached", "This instance is serving its maximum number of concurrent event streams. " ++
        "Close unused dashboard tabs or retry shortly. Operators: raise SSE_MAX_STREAMS or add replicas."),
    // ── WORKSPACE ────────────────────────────────────────────────────────────
    // ── BILLING ──────────────────────────────────────────────────────────────
    // ── AGENT ────────────────────────────────────────────────────────────────
    e("UZ-FLEETKEY-001", .not_found, "Fleet key not found", "Fleet key not found. Verify the fleet_key_id."),
    // ── WEBHOOK ──────────────────────────────────────────────────────────────
    e("UZ-WH-001", .not_found, "Fleet not found for webhook", "No fleet is registered for this webhook endpoint."),
    e("UZ-WH-002", .bad_request, "Malformed webhook", "Webhook payload could not be parsed. Check Content-Type and body."),
    // UZ-WH-003 retired (paused-ingress rework): a webhook to a paused fleet answers
    // 200 {"ignored":"fleet_paused"} — sender retry queues add no value for
    // an intentionally paused fleet. Steer ingress refuses with UZ-AGT-012.
    e("UZ-WH-010", .unauthorized, "Invalid webhook signature", "Webhook signature verification failed. Confirm the signing secret " ++
        "stored for this provider (Slack/Clerk/other) matches the one configured " ++
        "upstream."),
    e("UZ-WH-011", .unauthorized, "Stale webhook timestamp", "Webhook request timestamp is outside the allowed 5-minute drift window. " ++
        "This may indicate a replay attack or clock skew."),
    e("UZ-WH-020", .unauthorized, "Webhook credential not configured", "No webhook credential is configured for this fleet's source. Run " ++
        "`agentsfleet secret add <source> --data='{\"webhook_secret\":\"...\"}'` " ++
        "in the fleet's workspace, then resend."),
    e("UZ-WH-030", .payload_too_large, "Webhook payload too large", "Webhook body exceeds the 1 MiB ingest limit. Reduce the payload size " ++
        "or filter at the source."),
    // ── SLACK CONNECTOR ──────────────────────────────────────────────────────
    // Events ingress (M106 §2). 010/011 are the only rejections Slack ever sees
    // as a 4xx; 020 is a benign 200-ack no-op (an uninstalled team), so its
    // entry carries `.ok` — the code is a structured telemetry/log reason, never
    // an `hx.fail` wire status (the handler returns 200 via `hx.ok`).
    e("UZ-SLK-010", .unauthorized, "Invalid Slack signature", "The Slack request signature did not verify. Confirm the platform Slack " ++
        "app signing secret matches the one vaulted at slack-app/signing_secret."),
    e("UZ-SLK-011", .unauthorized, "Stale Slack timestamp", "The Slack request timestamp is outside the allowed 5-minute drift window — " ++
        "a replay attempt or a skewed server clock."),
    e("UZ-SLK-020", .ok, "Slack team not installed", "The Slack team that sent this event has no connector install, so the event " ++
        "is acknowledged (200) and ignored. Re-run Connect Slack in the dashboard to (re)install."),
    e("UZ-SLK-022", .bad_gateway, "Slack token exchange failed", "The Slack OAuth code could not be exchanged for a bot token. Retry the " ++
        "connect flow; if it persists, verify the platform Slack app credentials."),
    // 030 is log-only: the connector:outbound worker posts the answer async, so a
    // failed chat.postMessage is logged + retried with backoff, never surfaced to
    // an HTTP caller. The status is nominal (mirrors 022 — a Slack-upstream fault).
    e("UZ-SLK-030", .bad_gateway, "Slack answer post failed", "The channel bot's answer could not be delivered to Slack (missing chat:write, " ++
        "a 429, or a Slack outage). It is logged and retried with backoff; the run itself never fails."),
    // ── TOOL ─────────────────────────────────────────────────────────────────
    e("UZ-TOOL-005", .bad_request, "Unknown tool", "Unknown tool name. Check spelling against the known tools list."),
    // ── AGENT ───────────────────────────────────────────────────────────────
    e("UZ-AGT-003", .failed_dependency, "Fleet credential missing", "A required credential is not in the vault. Add it with: agentsfleet secret add <name>"),
    e("UZ-AGT-004", .internal_server_error, "Fleet claim failed", "Fleet could not be claimed from the database. Check that the fleet_id exists and status is 'active'."),
    e("UZ-AGT-006", .conflict, "Fleet name already exists", "A Fleet with this name already exists. Use 'agentsfleet kill <name>' first, then deploy again."),
    // UZ-AGT-007 retired (single-string credential body) → see UZ-VAULT-002.
    e("UZ-AGT-008", .bad_request, "Invalid fleet config", "Config JSON is malformed. Verify trigger, tools, credentials, and budget fields " ++
        "in your TRIGGER.md frontmatter. See tests/fixtures/fleetbundle/platform-ops/TRIGGER.md for a working example."),
    eu("UZ-AGT-009", .not_found, "Fleet not found", "Fleet not found. Verify the fleet_id and that it has not been killed.", "We couldn't find that Fleet. It may have been deleted, or the ID doesn't match one in this workspace."),
    e("UZ-AGT-010", .conflict, "Fleet state transition not allowed", "The requested lifecycle action is not valid from the fleet's current state. The response detail names the specific transition that was refused."),
    e("UZ-AGT-011", .bad_request, "SKILL.md and TRIGGER.md disagree on `name:`", "Top-level `name:` in SKILL.md must match `name:` in TRIGGER.md. One identity per Fleet Bundle."),
    e("UZ-AGT-012", .conflict, "Fleet is paused", "This fleet is not active and refuses new work. Resume it with: agentsfleet resume <fleet>, then retry."),
    // ── Fleet Bundle ───────────────────────────────────────────────────────
    eu("UZ-BUNDLE-001", .bad_request, "Invalid Fleet Bundle", "The supplied Fleet Bundle is missing SKILL.md or contains unsafe, oversized, or malformed files.", "That Fleet Bundle isn't valid. It's missing SKILL.md, or has an unsafe or oversized file — check the source and try again."),
    eu("UZ-BUNDLE-002", .not_found, "Fleet Bundle not found", "No installable library entry or stored snapshot matches the request in this workspace.", "We couldn't find that Fleet Bundle. It may not be installed in this workspace yet — check the Fleet library."),
    e("UZ-BUNDLE-003", .failed_dependency, "Fleet Bundle secrets missing", "Add the missing workspace secrets before installing this Fleet Bundle."),
    e("UZ-BUNDLE-004", .bad_gateway, "Fleet Bundle fetch failed", "The Fleet Bundle source could not be fetched from GitHub. The repository may be missing or private, or GitHub may be unreachable. Verify the source reference and retry."),
    e("UZ-BUNDLE-005", .service_unavailable, "Fleet Bundle storage unavailable", "Snapshot storage is not configured or is unavailable, so the validated bundle could not be stored. Retry later or contact the operator."),
    e("UZ-BUNDLE-006", .too_many_requests, "Too many Fleet Bundle imports in flight", "This instance is at its concurrent Fleet Bundle import ceiling. Honor the Retry-After header and retry with backoff."),
    // ── VAULT ────────────────────────────────────────────────────────────────
    eu("UZ-VAULT-001", .bad_request, "Secret data must be a non-empty JSON object", "POST /secrets body must include a 'data' field that is a JSON object with at least one key. " ++
        "Bare strings, arrays, scalars, and {} are rejected.", "That secret needs at least one field. Enter it as a JSON object with one or more keys — not a bare string or list."),
    eu("UZ-VAULT-002", .bad_request, "Secret data too large", "Stringified secret data exceeds 4KB. Compose the secret from fewer or shorter fields.", "That secret is too large. Keep it under 4 KB — trim or shorten the fields."),
    eu("UZ-VAULT-003", .not_found, "Secret not found", "No secret matches this name in the workspace. List the workspace secrets to find a valid name, or create it first.", "We couldn't find that secret. It may have already been deleted — refresh the list."),
    // ── PROVIDER (PUT /v1/tenants/me/provider) ───────────────────────────────
    eu("UZ-PROVIDER-001", .bad_request, "secret_ref required when mode=self_managed", "PUT body must include `secret_ref` naming a vault credential when `mode` is self_managed.", "Pick a secret to activate. Choose a stored secret before switching to a self-managed model."),
    eu("UZ-PROVIDER-002", .bad_request, "Secret row not found in vault", "The named secret_ref has no vault row in the tenant's primary workspace. " ++
        "Run `agentsfleet secret add <name> --data=@-` to create it.", "We couldn't find that secret. Store it under Secrets & ENVs, then try again."),
    eu("UZ-PROVIDER-003", .bad_request, "Secret JSON missing required field", "Stored secret JSON must include `provider` and `model` (non-empty strings); `api_key` is required for a named provider but optional for an `openai-compatible` endpoint. " ++
        "Re-run `agentsfleet secret add` with the required fields.", "That secret is missing required fields. It needs a provider and model set — edit it under Secrets & ENVs and add them."),
    eu("UZ-PROVIDER-004", .bad_request, "Model not in cached caps catalogue", "The effective model is not present in core.model_library. Pick a model from the model-caps endpoint " ++
        "or request the catalogue be extended.", "That model isn't in our catalogue yet. Pick a listed model, or ask us to add support for it."),
    e("UZ-PROVIDER-005", .bad_request, "Custom endpoint base_url invalid or unsafe", "An openai-compatible credential needs a valid `base_url`: it must use https and must not target a " ++
        "loopback, private, link-local, or cloud-metadata host. A non-openai-compatible provider must not carry a `base_url`."),
    e("UZ-PROVIDER-006", .not_found, "Catalogue model not found", "No core.model_library row matches this id. List the catalogue to find a valid id, or add the model first."),
    e("UZ-PROVIDER-007", .conflict, "Catalogue model is the active platform default", "This model is the active platform default. Point the default at another catalogued model before deleting it."),
    e("UZ-PROVIDER-008", .conflict, "Catalogue model already exists", "A catalogue row for this provider and model already exists. Edit the existing row instead of adding a duplicate."),
    eu("UZ-PROVIDER-009", .internal_server_error, "Platform LLM key not configured", "No active row in core.platform_llm_keys. An operator must set one via PUT /admin/platform-keys before tenants can switch to platform defaults.", "Platform defaults aren't set up on this deployment yet. Keep your current provider for now, or contact support."),
    // ── GATE ─────────────────────────────────────────────────────────────────
    // ── STARTUP ──────────────────────────────────────────────────────────────
    e("UZ-STARTUP-001", .internal_server_error, "Environment check failed", "Required environment variables are missing. Run 'agentsfleetd doctor' to see which ones."),
    e("UZ-STARTUP-002", .internal_server_error, "Config load failed", "Configuration failed to load. Check that all required env vars are set. " ++
        "Run 'agentsfleetd doctor' to verify."),
    e("UZ-STARTUP-003", .internal_server_error, "Database connect failed", "Database is unreachable. Check that DATABASE_URL is set and the database accepts connections."),
    e("UZ-STARTUP-004", .internal_server_error, "Redis connect failed", "Redis is unreachable. Check that REDIS_URL_API is set " ++
        "and the Redis server accepts connections. Run 'agentsfleetd doctor' to verify."),
    e("UZ-STARTUP-005", .internal_server_error, "Migration check failed", "Database migration state could not be verified. Check DB connectivity."),
    e("UZ-STARTUP-006", .internal_server_error, "Startup env allocation failed", "An environment variable could not be allocated at startup (out of memory). " ++
        "A required secret fails the boot closed; optional config falls back to its default — check host memory pressure."),
    // ── RUNNER (agentsfleet-runner /v1/runners control contract) ───────────────────
    e("UZ-RUN-001", .unauthorized, "Invalid runner token", "The Bearer runner_token is missing, malformed, or not recognized. Re-register the runner."),
    e("UZ-RUN-005", .conflict, "Stale fencing token", "The lease was reclaimed by a newer holder. This report is rejected; the current holder's result wins."),
    e("UZ-RUN-006", .not_found, "Lease not found", "No active lease matches this lease_id for the presenting runner; it may have expired, been reclaimed, or never existed."),
    e("UZ-RUN-007", .internal_server_error, "Sandbox establishment failed", "The runner could not establish the required sandbox (Landlock/cgroup/netns) for execution; the lease was refused fail-closed rather than run unconfined."),
    e("UZ-RUN-009", .unauthorized, "Runner admin state blocks access", "This runner is cordoned, draining, drained, or revoked and cannot call the runner plane. Re-enroll the host to mint a fresh runner token."),
    e("UZ-RUN-010", .conflict, "Lease exceeded max runtime", "The lease reached the hard maximum runtime and may not be renewed further; the run is terminated. The child is killed and the result, if any, is reported."),
    e("UZ-RUN-011", .conflict, "Lease lost", "The lease was reassigned to another runner before this renewal; it can no longer be renewed. The presenting runner must terminate its child."),
    e("UZ-RUN-012", .payment_required, "Lease renewal blocked: no credits", "The tenant's balance can no longer cover continued execution; the lease may not be renewed and the run terminates gracefully."),
    e("UZ-RUN-013", .bad_request, "Renew body malformed", "The renew request body could not be parsed; cumulative token counts default to zero and the slice meters its run-time fee only (never a negative charge). The lease is still renewed."),
    e("UZ-RUN-014", .not_found, "Runner not found", "No runner matches this runner_id. Verify the platform admin minted the runner before mutating it."),
    // Runtime / execute-path entries (sandbox, runner, relay, credentials,
    // approval-gate, memory, api-keys, grants, tool/credential, proxy,
    // gate-execute) live in error_entries_runtime.zig and are concatenated
    // into REGISTRY by error_registry.zig.
};
