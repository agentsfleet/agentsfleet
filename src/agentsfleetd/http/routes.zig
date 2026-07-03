//! The `Route` union — the central route type, extracted from router.zig to
//! keep both files under the 350-line cap (RULE FLL). router.zig re-exports it
//! (`pub const Route = routes.Route;`) so every `router.Route` consumer is
//! unchanged; the matching logic (match/matchV1) stays in router.zig.

const matchers = @import("route_matchers.zig");

pub const Route = union(enum) {
    healthz,
    readyz,
    metrics,
    // Public, unauthenticated model→cap catalogue served at a cryptic path
    // prefix (handlers/model_caps.zig). Both the install-skill and
    // `agentsfleet provider set` consume this once at provisioning time.
    model_caps,
    create_auth_session,
    /// GET /v1/auth/sessions/{session_id} — CLI polls for status + (post-
    /// approve) the public material it needs for the verify call. Never
    /// returns ciphertext (Invariant 1).
    poll_auth_session: []const u8,
    /// PATCH /v1/auth/sessions/{session_id}/approve — dashboard submits
    /// the ECDH ciphertext + verification code after the user clicks
    /// Approve. Clerk-authenticated.
    approve_auth_session: []const u8,
    /// POST /v1/auth/sessions/{session_id}/verify — CLI submits the 6-digit
    /// code; on success returns the encrypted JWT payload. Atomic Lua-EVAL
    /// transition verification_pending → consumed. No bearer auth — the
    /// code is the auth.
    verify_auth_session: []const u8,
    /// DELETE /v1/auth/sessions/{session_id} — explicit cancel by the
    /// session's owning Clerk user.
    delete_auth_session: []const u8,
    /// DELETE /v1/auth/sessions/all — abort every in-flight login session
    /// for the calling Clerk user.
    delete_all_auth_sessions,
    create_workspace,
    // Tenant-scoped billing snapshot — GET /v1/tenants/me/billing
    get_tenant_billing,
    // Tenant-scoped credit-pool charges (Usage tab) — GET /v1/tenants/me/billing/charges
    get_tenant_billing_charges,
    // Per-renewal slice breakdown behind one charge — carries {event_id}.
    // GET /v1/tenants/me/billing/charges/{event_id}/telemetry
    get_tenant_metering_periods: []const u8,
    // Tenant-scoped workspace list — GET /v1/tenants/me/workspaces
    list_tenant_workspaces,
    // Tenant-scoped LLM provider config — GET/PUT/DELETE /v1/tenants/me/provider
    tenant_provider,
    fleet_bundles, // GET /v1/fleets/bundles
    // Platform template onboarding — POST /v1/admin/fleet-templates
    // (platform-template:write). No workspace context.
    admin_fleet_templates,
    // Tenant template onboarding — POST /v1/workspaces/{ws}/fleet-templates
    // (template:write + workspace ownership). Carries workspace_id.
    workspace_fleet_templates: []const u8,
    /// POST /v1/webhooks/{fleet_id} — generic per-fleet webhook receiver.
    /// HMAC-only via webhook_sig middleware; secret resolved from the
    /// workspace credential keyed by the matching `triggers[].source`.
    receive_webhook: []const u8,
    // Clerk / Svix signed webhooks — /v1/webhooks/svix/{fleet_id}.
    receive_svix_webhook: []const u8,
    // Clerk user.created signup event — /v1/auth/identity-events/clerk.
    // Internal auth-plane endpoint (no fleet_id), kept out of /v1/webhooks/.
    auth_identity_event_clerk,
    // Fleet approval gate callback
    approval_webhook: []const u8,
    // Grant approval webhook — /v1/webhooks/{fleet_id}/grant-approval
    grant_approval_webhook: []const u8,
    /// POST /v1/webhooks/{fleet_id}/github — GitHub Actions ingest. HMAC via
    /// the workspace's `fleet:github` credential; handler filters to
    /// workflow_run/failure and XADDs the M42 envelope.
    github_webhook: []const u8,
    // Admin platform key management
    admin_platform_keys, // GET + PUT /v1/admin/platform-keys (method-dispatched in server.zig)
    delete_admin_platform_key: []const u8, // DELETE /v1/admin/platform-keys/{provider}
    // Admin model-caps catalogue CRUD (platform-admin)
    admin_models, // GET + POST /v1/admin/models (method-dispatched)
    admin_model_by_id: []const u8, // PATCH + DELETE /v1/admin/models/{uid}
    // Fleet create/read/update/delete (CRUD), workspace-scoped.
    workspace_fleets: []const u8, // GET|POST /v1/workspaces/{ws}/fleets
    patch_workspace_fleet: matchers.WorkspaceFleetRoute, // PATCH /v1/workspaces/{ws}/fleets/{id}
    workspace_credentials: []const u8, // GET|POST /v1/workspaces/{ws}/credentials
    workspace_credential: matchers.WorkspaceCredentialRoute, // PATCH|DELETE /v1/workspaces/{ws}/credentials/{name}
    // Chat ingress — POST /v1/workspaces/{ws}/fleets/{id}/messages
    workspace_fleet_messages: matchers.WorkspaceFleetRoute,
    // Per-Fleet event history + Server-Sent Events (SSE) live tail
    workspace_fleet_events: matchers.WorkspaceFleetRoute, // GET /v1/workspaces/{ws}/fleets/{id}/events
    workspace_fleet_events_stream: matchers.WorkspaceFleetRoute, // GET /v1/workspaces/{ws}/fleets/{id}/events/stream
    // Workspace-aggregate event history
    workspace_events: []const u8, // GET /v1/workspaces/{ws}/events
    // Approval inbox (workspace-scoped pending-gate surface)
    workspace_approvals: []const u8, // GET /v1/workspaces/{ws}/approvals
    workspace_approval_detail: matchers.ApprovalGateRoute, // GET /v1/workspaces/{ws}/approvals/{gate_id}
    workspace_approval_resolve: matchers.ApprovalResolveRoute, // POST /v1/workspaces/{ws}/approvals/{gate_id}:approve|:deny
    // External-fleet memory API — workspace-scoped resource collection (read-only).
    workspace_fleet_memories: matchers.WorkspaceFleetRoute, // GET (list-or-search); write verbs retired
    // Integration grant CRUD (workspace-scoped)
    request_integration_grant: matchers.WorkspaceFleetRoute, // POST /v1/workspaces/{ws}/fleets/{id}/integration-requests
    list_integration_grants: matchers.WorkspaceFleetRoute, // GET /v1/workspaces/{ws}/fleets/{id}/integration-grants
    revoke_integration_grant: matchers.WorkspaceFleetGrantRoute, // DELETE /v1/workspaces/{ws}/fleets/{id}/integration-grants/{grant_id}
    // Connector platform — one generic trio resolved against the comptime
    // registry (handlers/connectors/registry.zig); an unknown provider is a
    // 404 whose body names it. Slack + GitHub are registry ids, so their
    // shipped URLs are preserved verbatim.
    connector_connect: matchers.WorkspaceConnectorRoute, // POST /v1/workspaces/{ws}/connectors/{provider}/connect
    connector_status: matchers.WorkspaceConnectorRoute, // GET /v1/workspaces/{ws}/connectors/{provider}
    connector_callback: []const u8, // GET /v1/connectors/{provider}/callback (Bearer-less; state-authed)
    connector_catalog, // GET /v1/connectors?workspace_id={ws} (Bearer, connector:read) — registry-driven dashboard catalog
    // Slack events ingress — POST /v1/connectors/slack/events. Bearer-less;
    // the Slack v0 request signature is the only auth (in-handler). Bespoke:
    // inbound event surfaces are per-provider by nature.
    slack_events,
    // Workspace fleet-key management
    fleet_keys: []const u8, // POST|GET /v1/workspaces/{ws}/fleet-keys
    delete_fleet_key: matchers.WorkspaceFleetKeyRoute, // DELETE /v1/workspaces/{ws}/fleet-keys/{fleet_key_id}
    // Tenant API key CRUD.
    tenant_api_keys, // POST|GET /v1/api-keys
    tenant_api_key_by_id: []const u8, // PATCH|DELETE /v1/api-keys/{id}
    // Runner control plane — POST-only, identity from the Bearer token. register
    // is admin-gated; the self-plane verbs (heartbeat/lease/report/activity) are
    // gated by runnerBearer. `activity` is the only one with a path param
    // ({lease_id}); the rest resolve `me` from the token (no runner_id in path).
    register_runner, // POST /v1/runners
    fleet_runners_list, // GET /v1/fleets/runners (platform-admin operator-plane read)
    fleet_runner_patch: []const u8, // PATCH /v1/fleets/runners/{id}
    fleet_runner_events: []const u8, // GET /v1/fleets/runners/{id}/events
    fleet_streams_list, // GET /v1/fleets/streams (platform-admin — live SSE streams on this instance)
    runner_self, // GET /v1/runners/me (read-only — no last_seen bump)
    runner_heartbeat, // POST /v1/runners/me/heartbeats
    runner_lease, // POST /v1/runners/me/leases
    runner_report, // POST /v1/runners/me/reports
    runner_credentials_mint, // POST /v1/runners/me/credentials/mint (M102 — on-demand mint; lease_id in body)
    runner_activity: []const u8, // POST /v1/runners/me/leases/{lease_id}/activity
    runner_renew: []const u8, // POST /v1/runners/me/leases/{lease_id}/renew
    runner_memory_hydrate: []const u8, // GET /v1/runners/me/memory/{fleet_id}
    runner_memory_capture: []const u8, // POST /v1/runners/me/memory/{fleet_id}
    runner_bundle: []const u8, // GET /v1/runners/me/bundles/{content_hash}
};
