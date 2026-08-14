/// error_entries_runtime.zig — runtime / execute-path error entries.
///
/// Sibling of error_entries.zig (control-plane entries). Split for the
/// 350-line file cap. Both arrays are concatenated by error_registry.zig.
const entries = @import("error_entries.zig");
const Entry = entries.Entry;

const S_ALREADY_RESOLVED_USER_MSG = "Someone already resolved this. Refresh to see the outcome and who resolved it.";

// Entry constructors are single-sourced in error_entries.zig (Dimension 6.5).
const e = entries.e;
const eu = entries.eu;

pub const ENTRIES_RUNTIME = [_]Entry{
    // ── SANDBOX ──────────────────────────────────────────────────────────────
    // ── RUNNER ─────────────────────────────────────────────────────────────
    // UZ-EXEC-001 retired: no producer ever emitted it.
    // UZ-EXEC-002 retired: no producer ever emitted it.
    e("UZ-EXEC-003", .internal_server_error, "Run timed out", "The run exceeded its time limit and stopped."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-004", .internal_server_error, "Run memory limit reached", "The run reached its memory limit and stopped."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-005", .internal_server_error, "Run resource limit reached", "The run reached a resource limit and stopped."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-006", .internal_server_error, "Runner connection lost", "The connection to the runner was lost."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-007", .internal_server_error, "Run lease expired", "The run took too long and its lease expired."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-008", .internal_server_error, "Run stopped during renewal", "The run stopped because its lease was lost, expired, or refused renewal."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-009", .internal_server_error, "Runner security check failed", "Check the runner security settings before retrying."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-010", .internal_server_error, "Run crashed", "The run process crashed. Check the activity stream for details."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-011", .forbidden, "Landlock policy deny", "Landlock policy denied the filesystem operation."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-012", .internal_server_error, "Runner fleet init failed", "Runner fleet initialization failed. Check configuration."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-013", .internal_server_error, "Runner fleet run failed", "The runner could not finish the fleet run. Check the activity stream."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-014", .bad_request, "Run settings invalid", "The run settings are invalid. Check the fleet files before retrying."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-015", .payment_required, "Run stopped: fleet limit reached", "The run stopped because the fleet reached its configured limit."), // reachable: no — runner-engine internal FailureClass code, not dashboard-fetched
    e("UZ-EXEC-016", .unauthorized, "Runner token rejected", "The control plane rejected this host's runner token. Retrying cannot fix it: mint a fresh agt_r token, re-issue the host's runner token, and restart the runner."), // reachable: no — runner control-loop code, logged (server_stopped reason=token_rejected) not dashboard-fetched
    e("UZ-EXEC-017", .conflict, "Assignment exceeds host capability", "The assigned policy needs an enforcement mechanism this host cannot deliver. The runner row names it. Fix the host or relax the assignment."), // reachable: no — runner control-loop code, logged when the heartbeat verdict reads degraded
    // ── RELAY ────────────────────────────────────────────────────────────────
    // ── APPROVAL GATE ────────────────────────────────────────────────────────
    eu("UZ-APPROVAL-001", .bad_request, "Approval parse failed", "Gate policy in TRIGGER.md config_json has invalid syntax. Check the 'gates' section.", "That approval gate's config is invalid. Check the gates section in TRIGGER.md."),
    eu("UZ-APPROVAL-002", .not_found, "Approval not found", "That approval action was not found. It may have timed out or already been resolved.", "That approval action wasn't found. It may have already timed out or been resolved elsewhere."),
    eu("UZ-APPROVAL-003", .unauthorized, "Approval invalid signature", "The approval callback signature is invalid. Check the signing secret.", "That approval callback couldn't be verified. Check the signing secret configuration."),
    eu("UZ-APPROVAL-004", .service_unavailable, "Approval service unavailable", "The approval service is unavailable, so requests are denied by default. Check Redis connectivity.", "Approvals are temporarily unavailable. We deny requests while this service is down. Try again shortly."),
    eu("UZ-APPROVAL-005", .bad_request, "Approval condition invalid", "Gate condition expression is invalid. Supported operators: == and != with single-quoted values.", "That approval gate's condition is invalid. Check the gate's condition expression for a supported operator."),
    eu("UZ-APPROVAL-006", .conflict, "Approval already resolved", "Already resolved by Slack, the dashboard, or a timeout. The response body carries the outcome and resolver.", S_ALREADY_RESOLVED_USER_MSG),
    // ── MEMORY ───────────────────────────────────────────────────────────────
    e("UZ-MEM-002", .not_found, "Fleet not found for memory op", "The fleet_id does not exist or is not in this workspace. Verify both."), // reachable: no — runner memory-push endpoint (fleet-side), not fetched by ui/packages/app
    e("UZ-MEM-003", .service_unavailable, "Saved memory unavailable", "The memory backend is unreachable; the fleet falls back to ephemeral workspace memory. Check MEMORY_RUNTIME_URL."), // reachable: no — runner memory-push endpoint (fleet-side), not fetched by ui/packages/app
    eu("UZ-MEM-004", .not_found, "Memory entry not found", "No entry with that key exists for this fleet. List the fleet's memories to confirm the exact key.", "That memory entry is already gone — the fleet isn't holding anything under that key."),
    // ── AGENT KEYS (workspace-scoped, agt_a prefix) ────────────────────────────
    e("UZ-APIKEY-001", .unauthorized, "Invalid API key", "API key is invalid or revoked. Mint a replacement with: `POST /v1/api-keys`"), // reachable: no — tenant bearer auth (CLI/runner), not a browser session
    // ── TENANT API KEYS (tenant-scoped, agt_t prefix) ────────────────────────
    eu("UZ-APIKEY-003", .not_found, "API key not found", "No API key matches the supplied id for this tenant. Verify the id with: GET /v1/api-keys", "We couldn't find that API key. It may have already been deleted — refresh the list."),
    e("UZ-APIKEY-004", .unauthorized, "API key has been revoked", "This key was revoked and can no longer authenticate. Mint a replacement with: POST /v1/api-keys"), // reachable: no — CLI/API-key bearer-auth surface, not a browser session
    eu("UZ-APIKEY-005", .conflict, "Key name already exists in this tenant", "key_name must be unique per tenant. Pick a different name or revoke the existing key first.", "An API key with that name already exists. Pick a different name for this tenant."),
    eu("UZ-APIKEY-006", .conflict, "API key is already revoked", "This key is already revoked. No further action is required.", "That API key is already revoked. Refresh the list to see its current state."),
    eu("UZ-APIKEY-007", .conflict, "active cannot be set to true; mint a new key instead", "Re-activation is not supported. Create a new key via POST /v1/api-keys and revoke the old one.", "A revoked key can't be reactivated. Mint a new key instead."),
    eu("UZ-APIKEY-008", .conflict, "Active API key must be revoked before deletion", "Revoke the key first with `PATCH /v1/api-keys/{id}` body `{\"active\": false}`, then retry DELETE.", "This key is still active. Revoke it first, then delete it."),
    // ── INTEGRATION GRANTS ────────────────────────────────────────────────────
    // UZ-GRANT-001 restored (Jul 06, 2026): believed dead when M116 authored
    // its Dead Code Sweep — that grep matched the code STRING "UZ-GRANT-001",
    // which is correct for finding e()/eu() registry entries but blind to a
    // caller that references the derived ERR_* constant by name instead
    // (fleet/service.zig's grant-gate lease check, credentials_mint.zig's
    // on-demand mint gate — both landed in the grant-gated mint/lease PR,
    // merged concurrently with this branch). Restored verbatim; see this
    // spec's Discovery for the cross-PR collision this exposed.
    e("UZ-REPAIR-010", .forbidden, "Write mint requires an approved gate", "No repository-write approval was answered for this event, so no write-scoped token issues. The run continues read-only."), // reachable: no — response goes to the runner's credential-mint call, not to a dashboard fetch
    e("UZ-REPAIR-011", .forbidden, "Fleet binding changed since approval", "The fleet's repository binding no longer matches the approved card. Re-raise the approval so a human sees the current reach."), // reachable: no — runner-plane refusal, surfaced through the activity stream
    e("UZ-REPAIR-012", .ok, "Duplicate repair link refused", "A repair Pull Request already links this incident, so a second one is acknowledged and not recorded. Close the surplus Pull Request on GitHub."), // reachable: no — logged on the webhook arm; the delivery itself answers 200-ignored
    e("UZ-REPAIR-013", .forbidden, "Write request allowance exhausted", "This approval already funded 32 write-credential requests. Answer a new repository-write approval first."), // reachable: no — runner-plane refusal, surfaced through event history
    e("UZ-REPAIR-014", .ok, "Repair provenance refused", "The repair branch does not match an approved write gate on workspace, Fleet, event, installation, repository, and App author. The delivery is acknowledged and nothing is recorded."), // reachable: no — signed webhook delivery returns a named ignore reason
    e("UZ-GRANT-001", .forbidden, "No integration grant for service", "This fleet has no approved grant for the target service. Check it with `GET /v1/workspaces/{ws}/fleets/{id}/integration-grants` and resolve its approval."), // reachable: no — runner-only mint/lease gate, not fetched by ui/packages/app
    eu("UZ-GRANT-002", .not_found, "Integration grant not found", "No grant with that id exists for this fleet, or it was already revoked. List current grants with `GET /v1/workspaces/{ws}/fleets/{id}/integration-grants`.", "We couldn't find that grant request. It may have already been resolved — refresh the list."),
    eu("UZ-GRANT-003", .conflict, "Grant already resolved", "This grant was already approved or denied. The original decision stands; this request changed nothing.", S_ALREADY_RESOLVED_USER_MSG),
    // ── CREDENTIAL BROKER (M102 — on-demand mint) ─────────────────────────────
    // Surfaced first at POST /v1/runners/me/credentials/mint (the mint endpoint
    // is the first caller — registering them earlier would be caller-less, NDC).
    // No secret ever appears in these messages (VLT) — host/status only.
    eu("UZ-CRED-001", .not_found, "Integration not connected", "No connected integration matches this id in the fleet's workspace. Connect it from the dashboard first.", "That integration isn't connected. Connect it from the Integrations page, then try again."),
    e("UZ-CRED-002", .service_unavailable, "Credential broker not configured", "The on-demand credential broker is not configured on this deployment. An operator must set it up before runners can mint credentials."), // reachable: no — runner-only mint endpoint, not fetched by ui/packages/app
    e("UZ-GH-001", .conflict, "GitHub App reconnect required", "The GitHub App installation was uninstalled or revoked, so no token can be minted. Reconnect GitHub from the dashboard."), // reachable: no — response goes to the runner's credential-mint call, surfaced to the agent as a tool failure, not to a dashboard fetch
    e("UZ-GH-002", .bad_gateway, "GitHub token mint failed", "GitHub did not return an installation token. Retry shortly; if it continues, check GitHub status and the App configuration."), // reachable: no — response goes to the runner's credential-mint call, not to a dashboard fetch
    // ── CONNECTOR PLATFORM (the connect round-trip + bounded vendor calls) ────
    eu("UZ-CONN-001", .service_unavailable, "Connector not configured", "An operator must configure this provider app before workspaces can connect.", "This connector isn't set up yet. Contact your operator to enable it."),
    eu("UZ-CONN-002", .bad_request, "Invalid connect state", "The connect callback's state was missing, forged, expired, or already used. Start the connect again from the dashboard.", "That connection attempt expired or was already used. Start connecting again from the dashboard."),
    eu("UZ-CONN-003", .bad_gateway, "Connector vendor call exceeded its deadline", "An outbound provider call timed out or could not reach the provider. Retry once; if it continues, check provider status and network access.", "We couldn't reach that service right now. Try again shortly."),
    eu("UZ-CONN-004", .not_found, "Unknown connector provider", "The `{provider}` segment is not in this deployment's connector registry. Check the dashboard connectors page for the available providers.", "We don't recognize that connector. Check the available connectors on the dashboard."),
    eu("UZ-CONN-006", .bad_gateway, "Connector OAuth exchange failed", "The connector's OAuth exchange was rejected. Start the connect again; if it repeats, check the provider app credentials and redirect URL.", "That connection didn't go through. Try connecting again from the dashboard."),
    eu("UZ-CONN-007", .internal_server_error, "Connector catalog lookup failed", "The connector key lookup failed in the database. Retry; if it continues, check database connectivity.", "We couldn't load your connectors right now. Try refreshing — if it keeps failing, contact support."),
    eu("UZ-CONN-008", .forbidden, "Connector installation ownership not verified", "GitHub did not confirm you can access that App installation, or it is connected to another workspace. Retry while signed in to the owning GitHub account.", "We couldn't verify that this GitHub installation belongs to you. Sign in with the owning GitHub account and try again."),
};
