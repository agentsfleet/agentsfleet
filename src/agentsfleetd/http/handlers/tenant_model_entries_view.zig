//! GET /v1/tenants/me/models — list-view construction.
//!
//! Joins each `core.tenant_model_entries` row to its secret's non-secret
//! metadata (provider/kind/base_url/has_key), computes `active` against the
//! tenant's current `core.tenant_model_selection` row, and resolves
//! context/rates from the model library cache when known. Pure read — the
//! "every active selection has a matching entry" invariant is guaranteed at
//! activation-write time (tenant_provider.zig's ensureEntryForSelection), never
//! patched up here. Split out of tenant_model_entries.zig (the 4-endpoint
//! handler) per RULE FLL.
//!
//! THIS READ DECRYPTS NOTHING (the never-decrypt invariant). Every field it
//! displays is metadata that now lives in the `meta_*` columns
//! (`schema/036_vault_secret_metadata.sql`), written beside the ciphertext at
//! store time. One batch query answers the whole page.
//!
//! What that replaced: `projectEntry` called `secret_probe.loadTenantSecretJson`
//! per row, and each call resolved the primary workspace AND opened an AES-GCM
//! envelope. A 100-row page cost ~200 statements and 100 decryptions to render
//! a view whose every field is shown to any authorized caller. Now it costs one
//! workspace lookup and one metadata query, and no ciphertext is loaded at all —
//! so there is no plaintext to leak, mishandle, or forget to zero.

const std = @import("std");
const pg = @import("pg");

const entries_state = @import("../../state/tenant_model_entries.zig");
const tenant_provider = @import("../../state/tenant_provider.zig");
const secret_probe = @import("../../state/secret_probe.zig");
const vault = @import("../../state/vault.zig");
const metadata = @import("../../secrets/metadata.zig");
const model_rate_batch = @import("../../state/model_rate_batch.zig");
const pagination = @import("../pagination.zig");

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
    input_nanos_per_mtok: ?i64 = null,
    cached_input_nanos_per_mtok: ?i64 = null,
    output_nanos_per_mtok: ?i64 = null,
    active: bool,
    created_at: i64,
};

pub const PlatformDefaultView = struct {
    const Self = @This();

    provider: []u8,
    model: []u8,
    context_cap_tokens: u32,
    input_nanos_per_mtok: ?i64 = null,
    cached_input_nanos_per_mtok: ?i64 = null,
    output_nanos_per_mtok: ?i64 = null,

    pub fn deinit(self: *Self, alloc: std.mem.Allocator) void {
        alloc.free(self.provider);
        alloc.free(self.model);
    }
};

/// The cursor payload for this page. Field order IS the canonical JSON key
/// order (`http/pagination.zig`), so reordering these fields invalidates every
/// cursor already in flight — bump `CURSOR_VERSION` if that ever happens.
///
/// It carries `tenant_uuid` and `limit` as well as the sort key: a cursor is
/// bound to the query that produced it, so replaying one against a different
/// tenant or a different page size is rejected rather than silently answered.
pub const Cursor = struct {
    v: u8 = pagination.CURSOR_VERSION,
    created_at: i64,
    id: []const u8,
    tenant_uuid: []const u8,
    limit: u32,
};

pub const ListResult = struct {
    rows: []EntryView,
    /// Opaque cursor for the next page, or null on the last one. Owned by the
    /// same allocator as `rows`.
    next_cursor: ?[]const u8 = null,
    platform_default_available: bool,
    /// The active platform default's identity — the Models page renders the
    /// Default row's model/context from it. Omitted from the wire
    /// (`emit_null_optional_fields=false`) when no default is configured;
    /// `platform_default_available` derives from the same read, so the two
    /// can never disagree.
    platform_default: ?PlatformDefaultView = null,

    pub fn deinit(self: *ListResult, alloc: std.mem.Allocator) void {
        for (self.rows) |r| freeView(alloc, r);
        alloc.free(self.rows);
        if (self.next_cursor) |c| alloc.free(c);
        if (self.platform_default) |*dv| dv.deinit(alloc);
    }
};

/// Caller owns the result and must call `.deinit(alloc)`. Activation
/// (tenant_provider.zig) guarantees the selection always has a matching
/// entry row, so no synthesize-on-read exists here.
///
/// Statement budget, whatever the page size: one selection read, one entry
/// page, one workspace resolve, one metadata batch, one platform default, one
/// rate batch. Decryptions: zero. Both batches are set-oriented, so the count
/// is independent of `limit` — that independence is the property §3 pins, not
/// the number itself.
///
/// `after` is the decoded boundary from the caller's cursor, already checked
/// against the authenticated tenant and the requested limit — this function
/// trusts it, because only the handler can perform that comparison.
pub fn buildList(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    tenant_id: []const u8,
    limit: u32,
    after: ?entries_state.PageStart,
) !ListResult {
    var selection = try tenant_provider.activeSelfManagedRef(alloc, conn, tenant_id);
    defer if (selection) |*s| s.deinit(alloc);

    const page = try entries_state.listPage(alloc, conn, tenant_id, limit, after);
    const entries = page.rows;
    defer entries_state.deinitEntryList(entries, alloc);

    // One workspace resolve for the whole page, not one per row: the credentials
    // a tenant's entries reference all live in its primary workspace.
    const ws_id = try secret_probe.resolvePrimaryWorkspace(alloc, conn, tenant_id);
    defer alloc.free(ws_id);

    // Positional, one slot per entry — deliberately NOT deduplicated. One
    // credential can back several model rows, so an earlier draft collected the
    // distinct set and then scanned it per row to match each entry back. That
    // cost two O(n²) passes and two helpers to save a few repeated key names in
    // one query parameter. `key_name = ANY($2)` is indifferent to duplicates, so
    // asking positionally makes `meta[i]` belong to `entries[i]` by construction
    // and deletes the matching problem instead of solving it.
    const refs = try alloc.alloc([]const u8, entries.len);
    defer alloc.free(refs);
    for (entries, 0..) |e, i| refs[i] = e.secret_ref;

    const meta = try alloc.alloc(?vault.SecretMetadata, entries.len);
    defer alloc.free(meta);
    try vault.loadMetadata(alloc, conn, ws_id, refs, meta);
    defer vault.freeMetadata(alloc, meta);

    // Resolved BEFORE the rates, so the default's `(provider, model)` rides in
    // the same statement as the page's rather than costing a second one.
    //
    // Sequential reuse of `conn` is safe: every query above
    // (`activeSelfManagedRef`, `list`, `resolvePrimaryWorkspace`,
    // `loadMetadata`) fully drains its own result set before returning —
    // mirrors `fleets/secret_list.zig`. A failure reading the default degrades
    // to "no default known" rather than failing the list — the posture the
    // boolean always had.
    var source_default = tenant_provider.platformDefaultView(alloc, conn) catch null;
    errdefer if (source_default) |*d| d.deinit(alloc);

    var rates = try PageRates.load(alloc, conn, entries, meta, source_default);
    defer rates.deinit(alloc);

    var views: std.ArrayList(EntryView) = .empty;
    errdefer {
        for (views.items) |v| freeView(alloc, v);
        views.deinit(alloc);
    }
    for (entries, 0..) |e, i| {
        const active = if (selection) |s|
            std.mem.eql(u8, e.secret_ref, s.secret_ref) and std.mem.eql(u8, e.model_id, s.model)
        else
            false;
        const view = try projectEntry(alloc, e, active, meta[i], rates.slots[i]);
        errdefer freeView(alloc, view);
        try views.append(alloc, view);
    }

    // Takes ownership of the resolved default's strings — no dupe, so nothing
    // frees them twice.
    var platform_default: ?PlatformDefaultView = if (source_default) |d| .{
        .provider = d.provider,
        .model = d.model,
        .context_cap_tokens = d.context_cap_tokens,
        .input_nanos_per_mtok = if (rates.forDefault()) |r| r.input_nanos_per_mtok else null,
        .cached_input_nanos_per_mtok = if (rates.forDefault()) |r| r.cached_input_nanos_per_mtok else null,
        .output_nanos_per_mtok = if (rates.forDefault()) |r| r.output_nanos_per_mtok else null,
    } else null;
    errdefer if (platform_default) |*dv| dv.deinit(alloc);

    // The cursor is built from the LAST ENTRY ROW, not from the last view: the
    // seek predicate compares against `core.tenant_model_entries` columns, and
    // the view's fields are a projection that may not round-trip them.
    const next_cursor: ?[]const u8 = if (page.has_more and entries.len > 0)
        try pagination.encode(alloc, Cursor, .{
            .created_at = entries[entries.len - 1].created_at,
            .id = entries[entries.len - 1].id,
            .tenant_uuid = tenant_id,
            .limit = limit,
        })
    else
        null;
    errdefer if (next_cursor) |c| alloc.free(c);

    return .{
        .rows = try views.toOwnedSlice(alloc),
        .next_cursor = next_cursor,
        .platform_default_available = platform_default != null,
        .platform_default = platform_default,
    };
}

/// The page's rate lookups, answered in ONE statement whatever the page size.
///
/// Positional, exactly like the `meta` batch above and for the same reason:
/// `slots[i]` belongs to `entries[i]` by construction, and the platform default —
/// when there is one — takes the slot after the last entry. Folding the default
/// in here rather than resolving it on its own is what keeps the whole page at
/// one rate statement instead of two.
///
/// What this replaced: a resident-only cache read. It cost no statement, but it
/// answered null for every row until some unrelated billing charge happened to
/// load that exact `(provider, model)` — so after a restart the Models page
/// showed blank rates indefinitely. Nothing filled the cache for display any
/// more once the boot warm and the fixture `populate()` were removed.
const PageRates = struct {
    const Self = @This();

    slots: []?model_rate_batch.ModelRate,
    has_default: bool,

    fn load(
        alloc: std.mem.Allocator,
        conn: *pg.Conn,
        entries: []const entries_state.Entry,
        meta: []const ?vault.SecretMetadata,
        default: ?tenant_provider.PlatformDefaultView,
    ) !Self {
        const count = entries.len + @intFromBool(default != null);
        const providers = try alloc.alloc([]const u8, count);
        defer alloc.free(providers);
        const models = try alloc.alloc([]const u8, count);
        defer alloc.free(models);

        for (entries, 0..) |e, i| {
            // A row whose credential is gone (or not yet backfilled) carries no
            // provider, so it has no catalogue identity to ask about. The empty
            // pair matches nothing and leaves the slot null — the same blank
            // cell that row already renders for every other metadata field.
            providers[i] = if (meta[i]) |m| (m.provider orelse "") else "";
            models[i] = e.model_id;
        }
        if (default) |d| {
            providers[count - 1] = d.provider;
            models[count - 1] = d.model;
        }

        const slots = try alloc.alloc(?model_rate_batch.ModelRate, count);
        errdefer alloc.free(slots);
        try model_rate_batch.loadRatesForPairs(conn, providers, models, slots);
        return .{ .slots = slots, .has_default = default != null };
    }

    fn deinit(self: *Self, alloc: std.mem.Allocator) void {
        alloc.free(self.slots);
    }

    fn forDefault(self: Self) ?model_rate_batch.ModelRate {
        if (!self.has_default) return null;
        return self.slots[self.slots.len - 1];
    }
};

/// Build one wire row from an entry and its already-read projection.
///
/// No database handle and no tenant id: everything this needs was fetched in
/// bulk by `buildList`. That is the structural half of Invariant 5 — a function
/// with no connection cannot issue a query, so no future edit can quietly
/// reintroduce a per-row read here.
///
/// A missing credential (deleted out-of-band, or a row not yet backfilled)
/// degrades to an opaque custom_secret with no key, so the list still returns
/// 200 — mirroring `fleets/secret_list.zig`'s per-row resilience.
fn projectEntry(
    alloc: std.mem.Allocator,
    e: entries_state.Entry,
    active: bool,
    meta: ?vault.SecretMetadata,
    rate: ?model_rate_batch.ModelRate,
) !EntryView {
    const id = try alloc.dupe(u8, e.id);
    errdefer alloc.free(id);
    const model_id = try alloc.dupe(u8, e.model_id);
    errdefer alloc.free(model_id);
    const secret_ref = try alloc.dupe(u8, e.secret_ref);
    errdefer alloc.free(secret_ref);

    const m = meta orelse return .{
        .id = id,
        .model_id = model_id,
        .secret_ref = secret_ref,
        .kind = metadata.Kind.custom_secret.wire(),
        .has_key = false,
        .active = active,
        .created_at = e.created_at,
    };

    const provider = try dupeOpt(alloc, m.provider);
    errdefer if (provider) |v| alloc.free(v);
    const base_url = try dupeOpt(alloc, m.base_url);
    errdefer if (base_url) |v| alloc.free(v);

    return .{
        .id = id,
        .model_id = model_id,
        .secret_ref = secret_ref,
        .provider = provider,
        .kind = m.kind.wire(),
        .base_url = base_url,
        .has_key = m.has_key,
        .context_cap_tokens = if (rate) |r| r.context_cap_tokens else null,
        .input_nanos_per_mtok = if (rate) |r| r.input_nanos_per_mtok else null,
        .cached_input_nanos_per_mtok = if (rate) |r| r.cached_input_nanos_per_mtok else null,
        .output_nanos_per_mtok = if (rate) |r| r.output_nanos_per_mtok else null,
        .active = active,
        .created_at = e.created_at,
    };
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
