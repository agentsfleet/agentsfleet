//! Admin platform LLM key management — the platform default provider + model.
//!
//! Consumed by the platform-operator "/admin/models" dashboard (Platform Default
//! card). Gated by the `platform-key:{read,admin}` scope at the route
//! (route_scopes.zig → requireScope) — the middleware is the sole gate (no
//! handler-internal capability re-check), mirroring register_runner.
//!
//! PUT sets the one active default: it validates the (provider, model) is a
//! priced core.model_caps row (the billing spine — a free-text default would
//! silently bill run-fee-only), records model/base_url/context_cap, and
//! deactivates every other provider's row so exactly one row stays active. The
//! api_key itself is NOT in this row — it lives in the source workspace's vault
//! under the provider name; the resolver follows source_workspace_id into it.

const std = @import("std");
const clock = @import("common").clock;
const logging = @import("log");
const httpz = @import("httpz");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const error_codes = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const tenant_provider = @import("../../../state/tenant_provider.zig");
const model_caps_store = @import("../../../state/model_caps_store.zig");
const hx_mod = @import("../hx.zig");

const log = logging.scoped(.http);

pub const Context = common.Context;

// Row shape for GET /v1/admin/platform-keys response.
// Defined at module level so std.ArrayList(PlatformKeyRow) compiles in all build modes.
const S_PROVIDER_MUST_BE_1_32_CHARS = "provider must be 1–32 chars";
const S_ROLLBACK_FAILED = "platform_default_rollback_failed";
// Postgres SQLSTATE for foreign_key_violation. fk_platform_llm_keys_model trips
// it when a concurrent model-delete wins the race against this activation — the
// signal that lets us return a catalogue-miss 4xx instead of an opaque 500.
const PG_SQLSTATE_FK_VIOLATION = "23503";

const PlatformKeyRow = struct {
    provider: []const u8,
    source_workspace_id: []const u8,
    active: bool,
    updated_at: i64,
};

// ── PUT /v1/admin/platform-keys ─────────────────────────────────────────────
// Set the active platform default: provider + catalogued model + key source.
// Body: {"provider","source_workspace_id","model","base_url"?}
// context_cap is read from the catalogue row (authoritative), never the body.

const PutInput = struct {
    provider: []const u8,
    source_workspace_id: []const u8,
    model: []const u8,
    /// Custom OpenAI-compatible endpoint. Required iff provider is the
    /// openai-compatible id, forbidden for named providers (same pairing rule as
    /// self-managed credentials). Threaded to the resolver/runner dial.
    base_url: ?[]const u8 = null,
};

pub fn innerPutAdminPlatformKey(hx: hx_mod.Hx, req: *httpz.Request) void {
    // Gate is the `platform-key:admin` scope on this route (route_scopes.zig +
    // requireScope) — capability rides the token, so no handler-internal
    // re-check here (mirrors register_runner).
    const body = req.body() orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(PutInput, hx.alloc, body, .{}) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Malformed JSON");
        return;
    };
    defer parsed.deinit();
    const input = parsed.value;

    if (input.provider.len == 0 or input.provider.len > 32) {
        hx.fail(error_codes.ERR_INVALID_REQUEST, S_PROVIDER_MUST_BE_1_32_CHARS);
        return;
    }
    if (input.model.len == 0 or input.model.len > 256) {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "model must be 1–256 chars");
        return;
    }
    if (!common.requireUuidV7Id(hx.res, hx.req_id, input.source_workspace_id, "source_workspace_id")) return;

    // base_url ⇔ provider pairing (same SSRF/https rule as self-managed creds):
    // required for openai-compatible, forbidden for named providers.
    const validated_base_url = tenant_provider.validateCredentialEndpoint(input.provider, input.base_url) catch {
        hx.fail(error_codes.ERR_PROVIDER_BASE_URL_INVALID, "base_url invalid: openai-compatible needs an https SSRF-safe URL; a named provider must omit it");
        return;
    };

    const key_id = id_format.generatePlatformLlmKeyId(hx.alloc) catch {
        common.internalOperationError(hx.res, "Failed to generate platform key id", hx.req_id);
        return;
    };
    const now_ms = clock.nowMillis();

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!workspaceExists(hx, conn, input.source_workspace_id)) return;

    // Billing-spine guard: the default's (provider, model) MUST be a priced
    // catalogue row. The cap comes from that row — authoritative, no body drift.
    const cap = model_caps_store.capFor(conn, input.provider, input.model) orelse {
        hx.fail(error_codes.ERR_PROVIDER_MODEL_NOT_IN_CATALOGUE, "model is not a priced catalogue row for this provider; add it to /admin/models first");
        return;
    };

    if (!activateDefault(hx, conn, key_id, input, validated_base_url, cap, now_ms)) return;

    log.debug("admin_platform_default_set", .{ .provider = input.provider, .model = input.model });

    hx.ok(.ok, .{
        .provider = input.provider,
        .model = input.model,
        .source_workspace_id = input.source_workspace_id,
        .active = true,
        .request_id = hx.req_id,
    });
}

/// True iff source_workspace_id references an existing workspace; writes the
/// error response and returns false otherwise.
fn workspaceExists(hx: hx_mod.Hx, conn: anytype, workspace_id: []const u8) bool {
    var q = PgQuery.from(conn.query(
        "SELECT 1 FROM core.workspaces WHERE workspace_id = $1 LIMIT 1",
        .{workspace_id},
    ) catch {
        common.internalOperationError(hx.res, "Failed to check workspace existence", hx.req_id);
        return false;
    });
    defer q.deinit();
    if ((q.next() catch null) == null) {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "source_workspace_id does not reference an existing workspace");
        return false;
    }
    return true;
}

/// Upsert this provider as the active default and deactivate every other row, in
/// one transaction so exactly one row is ever active. Returns false (after
/// writing the error response) on failure.
fn activateDefault(
    hx: hx_mod.Hx,
    conn: anytype,
    key_id: []const u8,
    input: PutInput,
    base_url: ?[]const u8,
    cap: i32,
    now_ms: i64,
) bool {
    conn.begin() catch {
        common.internalOperationError(hx.res, "Failed to begin transaction", hx.req_id);
        return false;
    };
    activateDefaultTx(conn, key_id, input, base_url, cap, now_ms) catch |tx_err| {
        // Inspect the sqlstate BEFORE rollback (rollback issues a new command that
        // clears conn.err). A foreign_key_violation here means the catalogued
        // model was deleted between capFor and commit — the model-delete won the
        // race fk_platform_llm_keys_model guards. Surface that as the same
        // catalogue-miss the pre-flight returns, plus a distinct log line, rather
        // than an opaque 500 the admin can't diagnose.
        const model_deleted_race = if (conn.err) |pg_err| std.mem.eql(u8, pg_err.code, PG_SQLSTATE_FK_VIOLATION) else false;
        conn.rollback() catch |rb_err| log.warn(S_ROLLBACK_FAILED, .{ .err = @errorName(rb_err) });
        if (model_deleted_race) {
            log.warn("platform_default_model_deleted_race", .{ .provider = input.provider, .model = input.model });
            hx.fail(error_codes.ERR_PROVIDER_MODEL_NOT_IN_CATALOGUE, "the chosen model was removed from the catalogue before activation; re-add it or pick another model");
            return false;
        }
        log.warn("platform_default_set_failed", .{ .err = @errorName(tx_err) });
        common.internalOperationError(hx.res, "Failed to set platform default", hx.req_id);
        return false;
    };
    conn.commit() catch {
        conn.rollback() catch |rb_err| log.warn(S_ROLLBACK_FAILED, .{ .err = @errorName(rb_err) });
        common.internalOperationError(hx.res, "Failed to commit platform default", hx.req_id);
        return false;
    };
    return true;
}

fn activateDefaultTx(
    conn: anytype,
    key_id: []const u8,
    input: PutInput,
    base_url: ?[]const u8,
    cap: i32,
    now_ms: i64,
) !void {
    _ = try conn.exec(
        \\INSERT INTO core.platform_llm_keys
        \\  (id, provider, source_workspace_id, model, base_url, context_cap_tokens, active, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, true, $7, $7)
        \\ON CONFLICT (provider) DO UPDATE
        \\SET source_workspace_id = EXCLUDED.source_workspace_id,
        \\    model = EXCLUDED.model,
        \\    base_url = EXCLUDED.base_url,
        \\    context_cap_tokens = EXCLUDED.context_cap_tokens,
        \\    active = true,
        \\    updated_at = EXCLUDED.updated_at
    , .{ key_id, input.provider, input.source_workspace_id, input.model, base_url, cap, now_ms });
    // Exactly one active row: stand every other provider down. NULL their model
    // so an inactive row never pins fk_platform_llm_keys_model — otherwise a
    // deactivated provider's stale model would block deleting that catalogue row.
    _ = try conn.exec(
        "UPDATE core.platform_llm_keys SET active = false, model = NULL, updated_at = $1 WHERE active = true AND provider <> $2",
        .{ now_ms, input.provider },
    );
}

// ── DELETE /v1/admin/platform-keys/{provider} ────────────────────────────────
// Deactivate the platform default for a provider (sets active = false).

pub fn innerDeleteAdminPlatformKey(hx: hx_mod.Hx, req: *httpz.Request, provider: []const u8) void {
    _ = req;
    // Gate is the `platform-key:admin` scope (route_scopes.zig) — see PUT above.
    if (provider.len == 0 or provider.len > 32) {
        hx.fail(error_codes.ERR_INVALID_REQUEST, S_PROVIDER_MUST_BE_1_32_CHARS);
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    // NULL model alongside active=false so the deactivated row stops pinning
    // fk_platform_llm_keys_model (lets the admin delete that catalogue model).
    _ = conn.exec(
        "UPDATE core.platform_llm_keys SET active = false, model = NULL, updated_at = $1 WHERE provider = $2",
        .{ clock.nowMillis(), provider },
    ) catch {
        common.internalOperationError(hx.res, "Failed to deactivate platform key", hx.req_id);
        return;
    };

    log.debug("admin_platform_key_deactivated", .{ .provider = provider });

    hx.ok(.ok, .{
        .provider = provider,
        .active = false,
        .request_id = hx.req_id,
    });
}

// ── GET /v1/admin/platform-keys ──────────────────────────────────────────────
// List all platform key rows (active and inactive). Never returns key material.

pub fn innerGetAdminPlatformKeys(hx: hx_mod.Hx, req: *httpz.Request) void {
    _ = req;
    // Gate is the `platform-key:admin` scope (route_scopes.zig) — see PUT above.
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    var q = PgQuery.from(conn.query(
        "SELECT provider, source_workspace_id, active, updated_at FROM core.platform_llm_keys ORDER BY provider",
        .{},
    ) catch {
        common.internalOperationError(hx.res, "Failed to query platform keys", hx.req_id);
        return;
    });
    defer q.deinit();

    var rows: std.ArrayList(PlatformKeyRow) = .empty;

    while (true) {
        const maybe_row = q.next() catch |e| {
            log.err("admin_platform_keys_row_error", .{ .error_code = error_codes.ERR_INTERNAL_DB_QUERY, .err = @errorName(e) });
            break;
        };
        const row = maybe_row orelse break;
        const prov = hx.alloc.dupe(u8, row.get([]u8, 0) catch continue) catch continue;
        const src_ws = hx.alloc.dupe(u8, row.get([]u8, 1) catch continue) catch continue;
        const active = row.get(bool, 2) catch continue;
        const updated_at = row.get(i64, 3) catch continue;
        rows.append(hx.alloc, .{
            .provider = prov,
            .source_workspace_id = src_ws,
            .active = active,
            .updated_at = updated_at,
        }) catch continue;
    }

    hx.ok(.ok, .{
        .keys = rows.items,
        .request_id = hx.req_id,
    });
}
