//! Admin model-library CRUD — `model:{read,admin}`-gated management of the priced
//! model catalogue (core.model_library), the billing spine.
//!
//! Routes (gated by the `model:read` (GET) / `model:admin` (write) scope in
//! route_scopes.zig → requireScope — the middleware is the sole gate, mirroring
//! register_runner; no handler-internal capability check):
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
//! Every mutation runs inside the catalogue-generation transaction
//! (`state/model_catalogue_revision.zig`): it takes the singleton generation row
//! FOR UPDATE, changes the catalogue, increments the generation, and commits —
//! so the new rows and the generation describing them become visible together.
//! That generation is what the response cache keys on and what billing
//! reconciles against, so a mutation that skipped it would leave every replica
//! serving a page and pricing a slice from a catalogue state that no longer
//! exists, with nothing to detect the drift.
//!
//! The local rate cache and the catalogue page cache are then cleared. That is
//! prompt reclamation only: a sibling replica clears nothing and stays correct
//! because rate entries carry the old generation (every billing read compares
//! it) and page keys carry the old revision (every catalogue read misses them).

const std = @import("std");
const clock = @import("common").clock;
const logging = @import("log");
const httpz = @import("httpz");
const common = @import("../common.zig");
const error_codes = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const model_identity = @import("../../../types/model_identity.zig");
const model_rate_cache = @import("../../../state/model_rate_cache.zig");
const model_library_store = @import("../../../state/model_library_store.zig");
const revision_state = @import("../../../state/model_catalogue_revision.zig");
const hx_mod = @import("../hx.zig");

const log = logging.scoped(.http);

pub const Context = common.Context;

// Shared with the tenant registry write path — see types/model_identity.zig for
// why one home matters. The two routes bound the same field and used to disagree.
const PROVIDER_MAX = model_identity.PROVIDER_MAX;
const MODEL_ID_MAX = model_identity.MODEL_ID_MAX;
const S_PROVIDER_LEN = "provider must be 1–64 chars";
const S_MODEL_ID_LEN = "model_id must be 1–256 chars";
const S_CAP_POSITIVE = "context_cap_tokens must be > 0";
const S_RATES_NONNEG = "rates (input/cached/output nanos_per_mtok) must be >= 0";
const S_BODY_REQUIRED = "Request body required";
const S_MALFORMED_JSON = "Malformed JSON";
const S_MODEL_NOT_FOUND = "No catalogue model matches this uid";
const S_UID_FIELD = "uid";

/// Mutable caps/rates shared by create + update. provider/model_id are create-only
/// (the row identity), so PATCH parses `model_library_store.Rates` directly and POST
/// parses the flat ModelInput (rates + identity) below.
const RatesInput = model_library_store.Rates;

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

/// Drop this replica's cached rates and catalogue pages after a committed
/// mutation, so the next charge reloads and the next catalogue read rebuilds
/// rather than waiting for a generation check or bucket pressure. Cannot fail
/// and reads nothing: correctness comes from the generation stored with each
/// rate entry and the revision carried in each page key, not from this call.
fn invalidateCaches(hx: hx_mod.Hx) void {
    model_rate_cache.clear();
    if (hx.ctx.model_library_cache) |cache| cache.clear();
}

// ── GET /v1/admin/models ─────────────────────────────────────────────────────

pub fn innerGetAdminModels(hx: hx_mod.Hx, req: *httpz.Request) void {
    _ = req;
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const rows = model_library_store.listForAdmin(hx.alloc, conn) catch {
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

    var txn = revision_state.beginMutation(conn) catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer txn.abort();

    // ON CONFLICT DO NOTHING + affected-row count distinguishes create (1) from
    // a duplicate (provider, model_id) attempt (0) → 409, without inspecting the
    // driver's unique-violation error. A 409 returns through the deferred abort,
    // so a rejected create leaves the generation untouched.
    const affected = model_library_store.create(conn, .{
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

    _ = txn.commitBumped(now_ms) catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    invalidateCaches(hx);
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

    const now_ms = clock.nowMillis();
    var txn = revision_state.beginMutation(conn) catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer txn.abort();

    const affected = model_library_store.updateRates(conn, uid, in, now_ms) catch {
        common.internalOperationError(hx.res, "Failed to update catalogue model", hx.req_id);
        return;
    };
    if ((affected orelse 0) == 0) {
        hx.fail(error_codes.ERR_MODEL_CAP_NOT_FOUND, S_MODEL_NOT_FOUND);
        return;
    }

    _ = txn.commitBumped(now_ms) catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    invalidateCaches(hx);
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
    // closes). The default must be repointed first. A DB fault during this check
    // fails CLOSED (block the delete, respond internal-error) rather than
    // collapsing to "not referenced" and letting the live default be removed.
    var txn = revision_state.beginMutation(conn) catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer txn.abort();

    const referenced = model_library_store.isReferencedByActiveDefault(conn, uid) catch {
        common.internalOperationError(hx.res, "Failed to verify model reference", hx.req_id);
        return;
    };
    if (referenced) {
        hx.fail(error_codes.ERR_MODEL_CAP_IN_USE, "This model is the active platform default; repoint the default before deleting it");
        return;
    }

    const affected = model_library_store.remove(conn, uid) catch {
        common.internalOperationError(hx.res, "Failed to delete catalogue model", hx.req_id);
        return;
    };
    if ((affected orelse 0) == 0) {
        hx.fail(error_codes.ERR_MODEL_CAP_NOT_FOUND, S_MODEL_NOT_FOUND);
        return;
    }

    _ = txn.commitBumped(clock.nowMillis()) catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    invalidateCaches(hx);
    log.debug("admin_model_deleted", .{ .uid = uid });

    hx.noContent();
}
