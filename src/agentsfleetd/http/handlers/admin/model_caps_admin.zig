//! Admin model-caps CRUD — platform-admin-gated management of the priced model
//! catalogue (core.model_caps), the billing spine.
//!
//! Routes (gated by registry.platformAdmin() in route_table.zig — the middleware
//! is the sole gate, mirroring register_runner; no handler-internal role check):
//!   GET    /v1/admin/models        list every catalogue row (with uid)
//!   POST   /v1/admin/models        create a priced row
//!   PATCH  /v1/admin/models/{uid}  update caps/rates (provider+model_id are the
//!                                  immutable identity — change them by delete+add)
//!   DELETE /v1/admin/models/{uid}  remove a row, unless it is the active platform
//!                                  default's model (409 — repoint the default first)
//!
//! Rows are keyed by uid in the URL, not (provider, model_id): a model_id can
//! contain '/' (e.g. accounts/fireworks/models/kimi-k2.6), which a path segment
//! cannot carry. uid is a uuidv7 — opaque, slash-free, SQL-injection-checked.
//!
//! Every successful mutation calls model_rate_cache.populate(conn) so a rate
//! change is live with no restart. The cache owns its process-lifetime memory
//! internally (page_allocator); the handler passes only the connection, so no
//! request-scoped allocator can ever back the process-global cache.

const std = @import("std");
const clock = @import("common").clock;
const logging = @import("log");
const httpz = @import("httpz");
const common = @import("../common.zig");
const error_codes = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const model_rate_cache = @import("../../../state/model_rate_cache.zig");
const model_caps_store = @import("../../../state/model_caps_store.zig");
const hx_mod = @import("../hx.zig");

const log = logging.scoped(.http);

pub const Context = common.Context;

const PROVIDER_MAX = 64;
const MODEL_ID_MAX = 256;
const S_PROVIDER_LEN = "provider must be 1–64 chars";
const S_MODEL_ID_LEN = "model_id must be 1–256 chars";
const S_CAP_POSITIVE = "context_cap_tokens must be > 0";
const S_RATES_NONNEG = "rates (input/cached/output nanos_per_mtok) must be >= 0";
const S_BODY_REQUIRED = "Request body required";
const S_MALFORMED_JSON = "Malformed JSON";
const S_MODEL_NOT_FOUND = "No catalogue model matches this uid";
const S_UID_FIELD = "uid";

/// Mutable caps/rates shared by create + update. provider/model_id are create-only
/// (the row identity), so PATCH parses `model_caps_store.Rates` directly and POST
/// parses the flat ModelInput (rates + identity) below.
const RatesInput = model_caps_store.Rates;

const ModelInput = struct {
    provider: []const u8,
    model_id: []const u8,
    context_cap_tokens: i32,
    input_nanos_per_mtok: i64,
    cached_input_nanos_per_mtok: i64,
    output_nanos_per_mtok: i64,
};

fn ratesValid(hx: hx_mod.Hx, r: RatesInput) bool {
    if (r.context_cap_tokens <= 0) {
        hx.fail(error_codes.ERR_INVALID_REQUEST, S_CAP_POSITIVE);
        return false;
    }
    if (r.input_nanos_per_mtok < 0 or r.cached_input_nanos_per_mtok < 0 or r.output_nanos_per_mtok < 0) {
        hx.fail(error_codes.ERR_INVALID_REQUEST, S_RATES_NONNEG);
        return false;
    }
    return true;
}

/// Rebuild the process-global rate cache from the now-mutated table. Logged, not
/// fatal: the row is already committed, so a transient cache-rebuild failure must
/// not 500 a successful write — the next mutation (or a boot) reconciles it.
fn repopulateCache(conn: anytype) void {
    model_rate_cache.populate(conn) catch |err| {
        log.warn("model_rate_cache_repopulate_failed", .{ .err = @errorName(err) });
    };
}

// ── GET /v1/admin/models ─────────────────────────────────────────────────────

pub fn innerGetAdminModels(hx: hx_mod.Hx, req: *httpz.Request) void {
    _ = req;
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const rows = model_caps_store.listForAdmin(hx.alloc, conn) catch {
        common.internalOperationError(hx.res, "Failed to query model catalogue", hx.req_id);
        return;
    };

    hx.ok(.ok, .{ .models = rows, .request_id = hx.req_id });
}

// ── POST /v1/admin/models ────────────────────────────────────────────────────

pub fn innerPostAdminModel(hx: hx_mod.Hx, req: *httpz.Request) void {
    const body = req.body() orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, S_BODY_REQUIRED);
        return;
    };
    const parsed = std.json.parseFromSlice(ModelInput, hx.alloc, body, .{}) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, S_MALFORMED_JSON);
        return;
    };
    defer parsed.deinit();
    const in = parsed.value;

    if (in.provider.len == 0 or in.provider.len > PROVIDER_MAX) {
        hx.fail(error_codes.ERR_INVALID_REQUEST, S_PROVIDER_LEN);
        return;
    }
    if (in.model_id.len == 0 or in.model_id.len > MODEL_ID_MAX) {
        hx.fail(error_codes.ERR_INVALID_REQUEST, S_MODEL_ID_LEN);
        return;
    }
    if (!ratesValid(hx, .{
        .context_cap_tokens = in.context_cap_tokens,
        .input_nanos_per_mtok = in.input_nanos_per_mtok,
        .cached_input_nanos_per_mtok = in.cached_input_nanos_per_mtok,
        .output_nanos_per_mtok = in.output_nanos_per_mtok,
    })) return;

    const uid = id_format.allocUuidV7(hx.alloc) catch {
        common.internalOperationError(hx.res, "Failed to generate model id", hx.req_id);
        return;
    };
    const now_ms = clock.nowMillis();

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    // ON CONFLICT DO NOTHING + affected-row count distinguishes create (1) from
    // a duplicate (provider, model_id) attempt (0) → 409, without inspecting the
    // driver's unique-violation error.
    const affected = model_caps_store.create(conn, .{
        .uid = uid,
        .provider = in.provider,
        .model_id = in.model_id,
        .rates = .{
            .context_cap_tokens = in.context_cap_tokens,
            .input_nanos_per_mtok = in.input_nanos_per_mtok,
            .cached_input_nanos_per_mtok = in.cached_input_nanos_per_mtok,
            .output_nanos_per_mtok = in.output_nanos_per_mtok,
        },
    }, now_ms) catch {
        common.internalOperationError(hx.res, "Failed to create catalogue model", hx.req_id);
        return;
    };
    if ((affected orelse 0) == 0) {
        hx.fail(error_codes.ERR_MODEL_CAP_EXISTS, "A catalogue row for this provider and model already exists");
        return;
    }

    repopulateCache(conn);
    log.debug("admin_model_created", .{ .provider = in.provider, .model_id = in.model_id });

    hx.ok(.created, .{
        .uid = uid,
        .provider = in.provider,
        .model_id = in.model_id,
        .context_cap_tokens = in.context_cap_tokens,
        .input_nanos_per_mtok = in.input_nanos_per_mtok,
        .cached_input_nanos_per_mtok = in.cached_input_nanos_per_mtok,
        .output_nanos_per_mtok = in.output_nanos_per_mtok,
        .request_id = hx.req_id,
    });
}

// ── PATCH /v1/admin/models/{uid} ─────────────────────────────────────────────

pub fn innerPatchAdminModel(hx: hx_mod.Hx, req: *httpz.Request, uid: []const u8) void {
    if (!common.requireUuidV7Id(hx.res, hx.req_id, uid, S_UID_FIELD)) return;

    const body = req.body() orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, S_BODY_REQUIRED);
        return;
    };
    const parsed = std.json.parseFromSlice(RatesInput, hx.alloc, body, .{}) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, S_MALFORMED_JSON);
        return;
    };
    defer parsed.deinit();
    const in = parsed.value;
    if (!ratesValid(hx, in)) return;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const affected = model_caps_store.updateRates(conn, uid, in, clock.nowMillis()) catch {
        common.internalOperationError(hx.res, "Failed to update catalogue model", hx.req_id);
        return;
    };
    if ((affected orelse 0) == 0) {
        hx.fail(error_codes.ERR_MODEL_CAP_NOT_FOUND, S_MODEL_NOT_FOUND);
        return;
    }

    repopulateCache(conn);
    log.debug("admin_model_updated", .{ .uid = uid });

    hx.ok(.ok, .{ .uid = uid, .updated = true, .request_id = hx.req_id });
}

// ── DELETE /v1/admin/models/{uid} ────────────────────────────────────────────

pub fn innerDeleteAdminModel(hx: hx_mod.Hx, req: *httpz.Request, uid: []const u8) void {
    _ = req;
    if (!common.requireUuidV7Id(hx.res, hx.req_id, uid, S_UID_FIELD)) return;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    // Block deleting the model the active platform default points at — otherwise
    // the next platform-mode lease resolves a model with no priced catalogue row
    // and silently degrades to run-fee-only (the revenue leak this milestone
    // closes). The default must be repointed first.
    if (model_caps_store.isReferencedByActiveDefault(conn, uid)) {
        hx.fail(error_codes.ERR_MODEL_CAP_IN_USE, "This model is the active platform default; repoint the default before deleting it");
        return;
    }

    const affected = model_caps_store.remove(conn, uid) catch {
        common.internalOperationError(hx.res, "Failed to delete catalogue model", hx.req_id);
        return;
    };
    if ((affected orelse 0) == 0) {
        hx.fail(error_codes.ERR_MODEL_CAP_NOT_FOUND, S_MODEL_NOT_FOUND);
        return;
    }

    repopulateCache(conn);
    log.debug("admin_model_deleted", .{ .uid = uid });

    hx.noContent();
}
