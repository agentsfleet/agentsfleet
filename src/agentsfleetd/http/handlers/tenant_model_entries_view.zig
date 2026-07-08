//! GET /v1/tenants/me/models — list-view construction (M121 §2).
//!
//! Joins each `core.tenant_model_entries` row to its secret's non-secret
//! metadata (provider/kind/base_url/has_key) via `secret_metadata.project`,
//! computes `active` against the tenant's current `core.tenant_model_selection`
//! row, and resolves `context_cap_tokens` from the model-caps catalogue when
//! known. Pure read — the "every active selection has a matching entry"
//! invariant is guaranteed at activation-write time (tenant_provider.zig's
//! ensureEntryForSelection), never patched up here. Split out of
//! tenant_model_entries.zig (the 4-endpoint handler) per RULE FLL.

const std = @import("std");
const pg = @import("pg");

const entries_state = @import("../../state/tenant_model_entries.zig");
const tenant_provider = @import("../../state/tenant_provider.zig");
const secret_probe = @import("../../state/secret_probe.zig");
const secret_metadata = @import("fleets/secret_metadata.zig");
const model_rate_cache = @import("../../state/model_rate_cache.zig");

const S_API_KEY = "api_key";

/// One wire row for the `models` array. `kind` is a static `@tagName` slice
/// (never freed); the rest are heap-owned (see `freeView`). No `api_key`
/// field exists — `has_key` is the only signal a caller sees.
pub const EntryView = struct {
    id: []const u8,
    model_id: []const u8,
    secret_ref: []const u8,
    provider: ?[]const u8 = null,
    kind: []const u8,
    base_url: ?[]const u8 = null,
    has_key: bool,
    context_cap_tokens: ?u32 = null,
    active: bool,
    created_at: i64,
};

pub const ListResult = struct {
    rows: []EntryView,
    platform_default_available: bool,

    pub fn deinit(self: *ListResult, alloc: std.mem.Allocator) void {
        for (self.rows) |r| freeView(alloc, r);
        alloc.free(self.rows);
    }
};

/// Caller owns the result and must call `.deinit(alloc)`. Fetches the active
/// selection and the entry list once each — a pure read. Activation
/// (tenant_provider.zig) guarantees the selection always has a matching
/// entry row, so no synthesize-on-read exists here.
pub fn buildList(alloc: std.mem.Allocator, conn: *pg.Conn, tenant_id: []const u8) !ListResult {
    var selection = try tenant_provider.activeSelfManagedRef(alloc, conn, tenant_id);
    defer if (selection) |*s| s.deinit(alloc);

    const entries = try entries_state.list(alloc, conn, tenant_id);
    defer entries_state.deinitEntryList(entries, alloc);

    var views: std.ArrayList(EntryView) = .empty;
    errdefer {
        for (views.items) |v| freeView(alloc, v);
        views.deinit(alloc);
    }
    for (entries) |e| {
        const active = if (selection) |s|
            std.mem.eql(u8, e.secret_ref, s.secret_ref) and std.mem.eql(u8, e.model_id, s.model)
        else
            false;
        const view = try projectEntry(alloc, conn, tenant_id, e, active);
        errdefer freeView(alloc, view);
        try views.append(alloc, view);
    }

    return .{
        .rows = try views.toOwnedSlice(alloc),
        .platform_default_available = platformDefaultAvailable(alloc, conn),
    };
}

/// A vault load failure (secret deleted out-of-band, decrypt error) degrades
/// the row to an opaque custom_secret with no key — mirrors
/// `fleets/secret_list.zig`'s resilience so the list still returns 200.
fn projectEntry(alloc: std.mem.Allocator, conn: *pg.Conn, tenant_id: []const u8, e: entries_state.Entry, active: bool) !EntryView {
    const id = try alloc.dupe(u8, e.id);
    errdefer alloc.free(id);
    const model_id = try alloc.dupe(u8, e.model_id);
    errdefer alloc.free(model_id);
    const secret_ref = try alloc.dupe(u8, e.secret_ref);
    errdefer alloc.free(secret_ref);

    var parsed = secret_probe.loadTenantSecretJson(alloc, conn, tenant_id, e.secret_ref) catch {
        return .{
            .id = id,
            .model_id = model_id,
            .secret_ref = secret_ref,
            .kind = secret_metadata.Kind.custom_secret.wire(),
            .has_key = false,
            .active = active,
            .created_at = e.created_at,
        };
    };
    defer parsed.deinit();

    const p = secret_metadata.project(parsed.value);
    const provider = try dupeOpt(alloc, p.provider);
    errdefer if (provider) |v| alloc.free(v);
    const base_url = try dupeOpt(alloc, p.base_url);
    errdefer if (base_url) |v| alloc.free(v);

    return .{
        .id = id,
        .model_id = model_id,
        .secret_ref = secret_ref,
        .provider = provider,
        .kind = p.kind.wire(),
        .base_url = base_url,
        .has_key = hasNonEmptyApiKey(parsed.value),
        .context_cap_tokens = if (p.provider) |prov| lookupContextCap(prov, model_id) else null,
        .active = active,
        .created_at = e.created_at,
    };
}

fn hasNonEmptyApiKey(value: std.json.Value) bool {
    if (value != .object) return false;
    const v = value.object.get(S_API_KEY) orelse return false;
    return v == .string and v.string.len > 0;
}

fn lookupContextCap(provider: []const u8, model_id: []const u8) ?u32 {
    const entry = model_rate_cache.lookup_model_rate(provider, model_id) orelse return null;
    return entry.context_cap_tokens;
}

fn dupeOpt(alloc: std.mem.Allocator, s: ?[]const u8) !?[]const u8 {
    return if (s) |v| try alloc.dupe(u8, v) else null;
}

fn freeView(alloc: std.mem.Allocator, v: EntryView) void {
    alloc.free(v.id);
    alloc.free(v.model_id);
    alloc.free(v.secret_ref);
    if (v.provider) |p| alloc.free(p);
    if (v.base_url) |b| alloc.free(b);
    // v.kind is a static @tagName slice — not owned, never freed.
}

/// Sequential reuse of `conn` is safe here: every query above (`list`,
/// `activeSelfManagedRef`, each `loadTenantSecretJson`) fully drains and
/// closes its own result set before returning — mirrors the two-pass
/// pattern in `fleets/secret_list.zig`.
fn platformDefaultAvailable(alloc: std.mem.Allocator, conn: *pg.Conn) bool {
    var view = tenant_provider.platformDefaultView(alloc, conn) catch return false;
    if (view) |*v| {
        v.deinit(alloc);
        return true;
    }
    return false;
}
