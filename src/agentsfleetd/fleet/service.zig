//! agentsfleetd-side runner control-plane orchestration — the `lease` verb.
//!
//! `leaseNext` delegates assignment to `assign.select`: across all active
//! fleets it atomically CLAIMS one (sticky-preferred), then either reclaims an
//! expired holder's event or pulls a fresh one. The pre-execution billing +
//! gate pass (insert-received → resolve tenant/provider → balance gate → debit
//! receive → approval gate, plus the terminal `gate_blocked` writes for
//! non-retryable refusals) lives in `service_billing.zig` (RULE FLL split);
//! this file keeps the lease build: `secrets_map`/context-budget resolution
//! lifted from `executeInSandbox`, the `fleet.runner_leases` row carrying the
//! durable event envelope + the claim's monotonic `fencing_token`, and the
//! 200 response. A fleet that declares credentials never receives a lease
//! without them — a missing secret refuses the lease with a terminal row
//! instead of shipping a silent null map (RULE ESO).
//!
//! Faithful, non-atomic: the debit fires here (pre-execution estimate, never
//! re-charged at report). `inline` secrets only.
//!
//! Allocator: handlers run inside the per-request arena (`hx.alloc`). Every
//! resolution output (the claimed session, tenant id, resolved provider, parsed
//! secret bodies, lease id, the acquired envelope) is arena-scoped and
//! reclaimed when the request ends — `assign` already freed the decoded stream
//! event (owned by the Redis client's allocator) before returning.

const std = @import("std");
const logging = @import("log");

const hx_mod = @import("../http/handlers/hx.zig");
const common = @import("../http/handlers/common.zig");
const ec = @import("../errors/error_registry.zig");
const wire = @import("contract");
const protocol = wire.protocol;
const constants = @import("common");
const id_format = @import("../types/id_format.zig");
const assign = @import("assign.zig");
const affinity = @import("affinity.zig");
const billing = @import("service_billing.zig");
const lease_row = @import("service_lease_row.zig");
const FleetSession = @import("fleet_session.zig");
const secrets_resolve = @import("secrets_resolve.zig");
const context_resolve = @import("context_resolve.zig");
const rows = @import("event_rows.zig");
const tenant_provider = @import("../state/tenant_provider.zig");
const metrics_runner = @import("../observability/metrics_runner.zig");
const event_envelope = wire.event_envelope;
const execution_policy = wire.execution_policy;

const Hx = hx_mod.Hx;
const log = logging.scoped(.runner_lease);

/// The lease-row billing fields, defined alongside the row write in
/// `service_lease_row.zig` (RULE FLL split); aliased here so the billing
/// helpers keep naming the type.
const Billed = lease_row.Billed;

/// POST /v1/runners/me/leases — claim the next event across all active fleets
/// (sticky-preferred), bill it (or reuse a reclaim's billing), and hand back the
/// work + resolved policy. Always 200: a `LeasePayload` when there is work, else
/// `lease=null` + a backoff hint.
pub fn leaseNext(hx: Hx) void {
    const runner_id = hx.principal.runner_id orelse {
        hx.fail(ec.ERR_RUN_INVALID_RUNNER_TOKEN, "runner identity required");
        return;
    };
    const acq = assign.select(hx, runner_id) orelse return replyNoWork(hx);

    var session = FleetSession.claimFleet(hx.alloc, acq.fleet_id, hx.ctx.pool) catch |err| {
        log.debug("lease_claim_unavailable", .{ .fleet_id = acq.fleet_id, .err = @errorName(err) });
        releaseClaim(hx, acq.fleet_id, acq.fencing_token);
        return replyNoWork(hx);
    };
    defer session.deinit(hx.alloc);

    const billed = billing.resolveBilling(hx, &session, acq) orelse {
        releaseClaim(hx, acq.fleet_id, acq.fencing_token);
        return replyNoWork(hx);
    };

    issueLease(hx, runner_id, &session, acq, billed) catch |err| {
        log.err("lease_issue_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .fleet_id = acq.fleet_id, .err = @errorName(err) });
        common.internalDbError(hx.res, hx.req_id);
    };
}

/// Build the lease payload + persist the `fleet.runner_leases` row (with the
/// durable envelope + the claim's fencing token), then 200.
fn issueLease(hx: Hx, runner_id: []const u8, session: *FleetSession, acq: assign.Acquired, billed: Billed) !void {
    // Provider key for the lease: a FRESH lease carried it from billing (bill key
    // == deliver key, no second resolve); a reclaim has no billing pass, so
    // re-resolve now (the key is never persisted to the lease row). deinit
    // (secureZero + free) runs after `hx.ok` serializes; set up first so the
    // defer also covers the early-return paths below.
    var resolved: ?tenant_provider.ResolvedProvider = billed.provider;
    if (resolved == null) resolved = resolveProviderForLease(hx, billed.tenant_id);
    defer if (resolved) |*r| r.deinit(hx.alloc);

    const ev_type = event_envelope.EventType.fromSlice(acq.event_type) orelse {
        log.warn("lease_unknown_event_type", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = acq.fleet_id, .event_type = acq.event_type });
        releaseClaim(hx, acq.fleet_id, acq.fencing_token);
        return replyNoWork(hx);
    };
    // Resolve declared secrets BEFORE building the lease: a missing credential
    // refuses the lease with a terminal row (RULE ESO — no lease ships with a
    // silent null secrets map); a transient vault/DB failure refuses without a
    // terminal write so the delivery stays leasable (RULE ECL). Entries are
    // arena-scoped and serialized synchronously by `hx.ok`.
    const secret_entries: ?[]secrets_resolve.ResolvedSecret = blk: {
        if (session.config.credentials.len == 0) break :blk null;
        break :blk secrets_resolve.resolveSecretsMap(hx.alloc, hx.ctx.pool, session.workspace_id, session.config.credentials) catch |err| {
            if (err == error.CredentialNotFound) {
                log.warn("lease_secret_missing", .{ .error_code = ec.ERR_AGENTSFLEET_CREDENTIAL_MISSING, .fleet_id = acq.fleet_id, .event_id = acq.event_id });
                billing.blockEvent(hx, acq.fleet_id, acq.event_id, rows.LABEL_SECRET_MISSING);
            } else {
                log.warn("lease_secrets_resolve_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = acq.fleet_id, .event_id = acq.event_id, .err = @errorName(err) });
            }
            releaseClaim(hx, acq.fleet_id, acq.fencing_token);
            return replyNoWork(hx);
        };
    };
    const envelope = event_envelope{
        .event_id = acq.event_id,
        .fleet_id = acq.fleet_id,
        .workspace_id = acq.workspace_id,
        .actor = acq.actor,
        .event_type = ev_type,
        .request_json = acq.request_json,
        .created_at = acq.event_created_at,
    };

    const lease_id = try id_format.generateRunnerLeaseId(hx.alloc);
    try lease_row.insertLeaseRow(hx, runner_id, acq, billed, lease_id);
    metrics_runner.incRunnerActiveLeases(runner_id); // in-memory gauge; decremented on the runner's report

    log.debug("lease_issued", .{ .fleet_id = acq.fleet_id, .event_id = acq.event_id, .lease_id = lease_id, .fencing_token = acq.fencing_token, .runner_id = runner_id, .kind = @tagName(acq.kind) });
    hx.ok(.ok, protocol.LeaseResponse{
        .lease = .{
            .lease_id = lease_id,
            .fencing_token = acq.fencing_token,
            .lease_expires_at = acq.leased_until,
            .secret_delivery = .@"inline",
            .event = envelope,
            .policy = resolveExecutionPolicy(hx, session, resolved, secret_entries),
            // The installed SKILL.md body (extracted by FleetSession), so the runner
            // delivers it to NullClaw. `claimFleet` resolves the session before the
            // fresh/reclaim split, so this is set identically on both paths. Borrowed
            // from `session`, which lives until the response serialises (deinit defer).
            .instructions = session.instructions,
            // Bundle-backed fleets carry the content hash so the runner downloads +
            // materializes the canonical snapshot; null for paste-installed fleets.
            .bundle = if (session.bundle_content_hash) |hash| .{ .content_hash = hash } else null,
        },
    });
}

/// Resolve the tenant's active provider+key for the lease. Called for BOTH
/// fresh and reclaim leases: `runBilling` discards the key it resolves for
/// metering and the lease row never persists it (plaintext secret in a table is
/// forbidden), so the key must be (re-)resolved here. Reclaim reuses its prior
/// billing — this resolve never re-charges. Returns null on resolve failure;
/// the lease then carries no key and the engine surfaces a clean config error.
/// Caller owns the result and must `deinit` (secureZero) after `hx.ok`.
fn resolveProviderForLease(hx: Hx, tenant_id: []const u8) ?tenant_provider.ResolvedProvider {
    const conn = hx.ctx.pool.acquire() catch |err| {
        log.warn("lease_provider_acquire_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
        return null;
    };
    defer hx.ctx.pool.release(conn);
    return tenant_provider.resolveActiveProvider(hx.alloc, conn, tenant_id) catch |err| {
        log.warn("lease_provider_key_resolve_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
        return null;
    };
}

/// `secrets_map` (inline, pre-resolved parsed bodies from `issueLease`) +
/// context budget + the resolved provider+key — the resolution
/// `executeInSandbox` does per execution, lifted onto the lease wire. Secret
/// bodies and the provider key are arena-scoped and serialized synchronously
/// by `hx.ok`; they are never logged (Invariant: no secret bytes in logs).
/// `resolved` is owned by the caller and outlives `hx.ok`. Resolution failures
/// refused the lease upstream — by here `entries` is complete or absent.
fn resolveExecutionPolicy(hx: Hx, session: *FleetSession, resolved: ?tenant_provider.ResolvedProvider, entries: ?[]secrets_resolve.ResolvedSecret) execution_policy.ExecutionPolicy {
    const alloc = hx.alloc;
    // Lease-time overlay (see user_flow.md): sentinel frontmatter (cap 0 /
    // model "") inherits the cap+model the control plane resolved into
    // tenant_providers; a real frontmatter value wins. `resolved` outlives the
    // hx.ok serialization (deinit deferred in issueLease), so the borrowed model
    // is valid for the response. No resolved provider ⇒ 0/"" ⇒ overlay no-op.
    const budget = context_resolve.resolveContextBudget(
        session.config.context,
        session.config.model,
        if (resolved) |r| r.context_cap_tokens else 0,
        if (resolved) |r| r.model else "",
    );
    var secrets_map: ?std.json.Value = null;
    if (entries) |list| {
        var obj: std.json.ObjectMap = .empty;
        for (list) |entry| {
            obj.put(alloc, entry.name, entry.parsed.value) catch |err| log.warn("lease_secret_put_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
        }
        secrets_map = .{ .object = obj };
    }
    const endpoint = customEndpoint(alloc, resolved);
    return .{
        .secrets_map = secrets_map,
        .context = budget,
        .provider = endpoint.provider,
        .api_key = if (resolved) |r| r.api_key else "",
        .inference_host = endpoint.inference_host,
        .base_url = endpoint.base_url,
    };
}

/// The lease's provider name + egress host + dialed URL, branching on whether the
/// resolved credential is a custom OpenAI-compatible endpoint:
///   - custom (base_url set): hand nullclaw the `custom:<url>` provider name (so
///     it classifies as `.compatible_provider` and honours the URL override —
///     NEVER "openai"), carry the URL as `base_url`, and derive the egress
///     `inference_host` from the SAME URL so the allowlist permits exactly it.
///   - named provider (base_url null): pass the provider through unchanged with
///     no base_url; `inference_host` stays "" exactly as before — named-provider
///     leases are byte-for-byte unchanged (Invariant 7).
/// Arena-scoped (`alloc` is `hx.alloc`); the `custom:<url>` name + host live until
/// `hx.ok` serializes. An OOM building the custom name degrades to the SAME shape
/// the named-provider branch returns — the raw provider with NO base_url and an
/// empty inference_host — so nullclaw never receives the bare `openai-compatible`
/// id paired with a URL (an undefined route: `classifyProvider` maps it to no
/// documented provider). With no base_url it classifies as a plain unknown named
/// provider and the engine fails authentication predictably, matching the clean
/// failure of the `resolved == null` / no-custom-endpoint branches above.
fn customEndpoint(
    alloc: std.mem.Allocator,
    resolved: ?tenant_provider.ResolvedProvider,
) struct { provider: []const u8, base_url: ?[]const u8, inference_host: []const u8 } {
    const r = resolved orelse return .{ .provider = "", .base_url = null, .inference_host = "" };
    const base_url = r.base_url orelse return .{ .provider = r.provider, .base_url = null, .inference_host = "" };

    const custom_name = std.fmt.allocPrint(alloc, "{s}{s}", .{ execution_policy.CUSTOM_PROVIDER_PREFIX, base_url }) catch {
        log.warn("lease_custom_provider_name_alloc_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .inference_host = execution_policy.hostFromUrl(base_url) });
        return .{ .provider = r.provider, .base_url = null, .inference_host = "" };
    };
    return .{ .provider = custom_name, .base_url = base_url, .inference_host = execution_policy.hostFromUrl(base_url) };
}

/// Free the affinity claim won by `assign` when this lease cannot be issued
/// (claim/billing failure), so the fleet is not stuck claimed until its TTL.
/// Token-guarded: frees the slot only while this claim's token is still live.
fn releaseClaim(hx: Hx, fleet_id: []const u8, token: u64) void {
    const conn = hx.ctx.pool.acquire() catch return;
    defer hx.ctx.pool.release(conn);
    affinity.release(conn, fleet_id, token) catch |err| {
        log.warn("lease_claim_release_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = fleet_id, .err = @errorName(err) });
    };
}

fn replyNoWork(hx: Hx) void {
    hx.ok(.ok, protocol.LeaseResponse{ .lease = null, .retry_after_ms = constants.NO_WORK_RETRY_AFTER_MS });
}

// `customEndpoint` only reads `provider` / `base_url`, so the test builds a
// ResolvedProvider from borrowed literals (api_key/model are unused here) and
// never deinits it — no allocation owns these bytes.
fn fixedProvider(provider: []const u8, base_url: ?[]const u8) tenant_provider.ResolvedProvider {
    return .{
        .mode = .self_managed,
        .provider = @constCast(provider),
        .api_key = @constCast(""),
        .model = @constCast(""),
        .context_cap_tokens = 0,
        .base_url = if (base_url) |u| @constCast(u) else null,
    };
}

test "customEndpoint: no resolved provider yields an empty, no-endpoint result" {
    const out = customEndpoint(std.testing.allocator, null);
    try std.testing.expectEqualStrings("", out.provider);
    try std.testing.expect(out.base_url == null);
    try std.testing.expectEqualStrings("", out.inference_host);
}

test "customEndpoint: a named provider passes through with no base_url" {
    const out = customEndpoint(std.testing.allocator, fixedProvider("anthropic", null));
    try std.testing.expectEqualStrings("anthropic", out.provider);
    try std.testing.expect(out.base_url == null);
    try std.testing.expectEqualStrings("", out.inference_host);
}

test "customEndpoint: a custom endpoint becomes the custom: provider name + egress host" {
    const out = customEndpoint(std.testing.allocator, fixedProvider(
        tenant_provider.OPENAI_COMPATIBLE_PROVIDER,
        "https://vllm.corp/v1",
    ));
    defer std.testing.allocator.free(out.provider); // the only allocated field
    try std.testing.expectEqualStrings("custom:https://vllm.corp/v1", out.provider);
    try std.testing.expectEqualStrings("https://vllm.corp/v1", out.base_url.?);
    try std.testing.expectEqualStrings("vllm.corp", out.inference_host);
}

test "customEndpoint: an OOM building the custom name fails predictably (no base_url smuggled)" {
    // failing_allocator OOMs the allocPrint; the branch must degrade to the
    // named-provider shape — the bare provider with NO base_url and an empty
    // host — so nullclaw never receives `openai-compatible` paired with a URL
    // (an undefined route). This is the clean failure the doc comment promises.
    const out = customEndpoint(std.testing.failing_allocator, fixedProvider(
        tenant_provider.OPENAI_COMPATIBLE_PROVIDER,
        "https://vllm.corp/v1",
    ));
    try std.testing.expectEqualStrings(tenant_provider.OPENAI_COMPATIBLE_PROVIDER, out.provider);
    try std.testing.expect(out.base_url == null);
    try std.testing.expectEqualStrings("", out.inference_host);
}
