//! Zombie memory adapter — wires NullClaw's postgres backend with per-zombie
//! row-level isolation via instance_id.
//!
//! ## The instance_id gap
//!
//! NullClaw's `memory_mod.initRuntime()` does not propagate `config.instance_id`
//! through `registry.resolvePaths()` into `BackendConfig.instance_id`. The
//! postgres factory (`createPostgres`) reads `cfg.instance_id` from BackendConfig,
//! but resolvePaths never sets it. Every zombie would get `instance_id = ""`
//! and could read each other's entries.
//!
//! This module bypasses the gap by directly building a `BackendConfig` with
//! `instance_id = cfg.namespace` before calling the postgres factory. The
//! NullClaw postgres engine then scopes every INSERT/SELECT/DELETE by that
//! instance_id, enforcing row-level isolation at the query layer.
//!
//! ## Scope (Step 4 — executor only)
//!
//! This file lives in `src/executor/` because the executor is a separate Zig
//! module (`src/executor/main.zig`) that cannot import from `src/memory/`.
//! Steps 5-7 (export/import, HTTP handlers) will create `src/memory/zombie_memory.zig`
//! for the main binary. The two files will share the adapter pattern.
//!
//! RULE FLS: all pg.Result rows from NullClaw are drained inside PostgresMemory.
//! RULE CTX: row-level isolation is at the query layer (instance_id scope), not
//! the process layer. The process-level boundary (memory_runtime role) is the
//! security guarantee; instance_id is the logical isolation within that role.

const std = @import("std");
const nullclaw = @import("nullclaw");
const memory_mod = nullclaw.memory;
const registry = memory_mod.registry;

const types = @import("types.zig");
const MemoryBackendConfig = types.MemoryBackendConfig;

const log = std.log.scoped(.executor_zombie_memory);

/// Build a NullClaw MemoryRuntime for a zombie with per-zombie row isolation.
///
/// - Connects via `cfg.connection` (memory_runtime role DSN)
/// - Sets `instance_id = cfg.namespace` ("zmb:{zombie_uuid}") on every query
/// - Target: `memory.memory_entries` (NullClaw auto-migrates schema on first connect)
/// - Profile: postgres_keyword — keyword recall, no vector search
///
/// Returns null on any error (postgres disabled, connection failure, OOM).
/// Caller falls back to ephemeral workspace SQLite and logs the failure.
pub fn initRuntime(
    alloc: std.mem.Allocator,
    cfg: *const MemoryBackendConfig,
    workspace_path: []const u8,
) ?memory_mod.MemoryRuntime {
    const desc = memory_mod.findBackend("postgres") orelse {
        log.warn("zombie_memory.backend_disabled: postgres not enabled in this build", .{});
        return null;
    };

    // dupeZ the connection string. PostgresMemory.init() passes it to libpq
    // (which copies it internally), so we can free url_z after init.
    const url_z = alloc.dupeZ(u8, cfg.connection) catch {
        log.warn("zombie_memory.oom: failed to allocate connection string", .{});
        return null;
    };
    defer alloc.free(url_z);

    // BackendConfig bypasses resolvePaths to set instance_id directly.
    // NullClaw's createPostgres calls applyPostgresConnectTimeout on the URL
    // and passes instance_id to PostgresMemory.init (which stores it for
    // every subsequent query).
    const backend_cfg = registry.BackendConfig{
        .db_path = null,
        .workspace_dir = workspace_path,
        .postgres_url = url_z.ptr,
        .postgres_schema = "memory",
        .postgres_table = "memory_entries",
        .instance_id = cfg.namespace, // "zmb:{zombie_uuid}" — row-level scope key
    };

    const instance = desc.create(alloc, backend_cfg) catch |err| {
        log.warn("zombie_memory.connect_failed err={s} namespace={s}", .{ @errorName(err), cfg.namespace });
        return null;
    };

    log.info("zombie_memory.ready namespace={s}", .{cfg.namespace});

    // Minimal MemoryRuntime for postgres_keyword: no search engine, no vector
    // store, no response cache. Fields with struct-level defaults (null, .on,
    // etc.) are omitted for brevity.
    return memory_mod.MemoryRuntime{
        .memory = instance.memory,
        .session_store = instance.session_store,
        .response_cache = null,
        .capabilities = desc.capabilities,
        .resolved = .{
            .primary_backend = "postgres",
            .retrieval_mode = "keyword",
            .vector_mode = "none",
            .embedding_provider = "none",
            .rollout_mode = "on",
            .vector_sync_mode = "best_effort",
            .hygiene_enabled = false,
            .snapshot_enabled = false,
            .cache_enabled = false,
            .semantic_cache_enabled = false,
            .summarizer_enabled = false,
            .source_count = 0,
            .fallback_policy = "degrade",
        },
        ._db_path = null,
        ._cache_db_path = null,
        ._engine = null,
        ._allocator = alloc,
        ._search_enabled = false, // keyword mode: bypass engine, use memory.recall() directly
    };
}

// ── Tests ────────────────────────────────────────────────────────────────────

test "zombie_memory: empty connection string returns null (no panic)" {
    // An empty connection string is always rejected — libpq cannot connect to "".
    // initRuntime must return null, not panic or reach unreachable.
    // In CI without -Dengines=postgres the backend lookup returns null before
    // even attempting a connection. Either way the outcome is null.
    const alloc = std.testing.allocator;
    const cfg = MemoryBackendConfig{
        .backend = "postgres",
        .connection = "", // empty — libpq rejects this at the URL-parse stage
        .namespace = "zmb:0195b4ba-8d3a-7f13-8abc-000000000001",
    };
    const rt = initRuntime(alloc, &cfg, "/tmp");
    // Empty connection can never succeed — assert null, not "either is fine".
    try std.testing.expect(rt == null);
}

test "zombie_memory: MemoryBackendConfig validates namespace format" {
    const good = MemoryBackendConfig{
        .backend = "postgres",
        .connection = "postgresql://memory_runtime:pw@localhost/testdb",
        .namespace = "zmb:0195b4ba-8d3a-7f13-8abc-000000000001",
    };
    try good.validate();

    const bad_ns = MemoryBackendConfig{
        .backend = "postgres",
        .connection = "postgresql://x@localhost/testdb",
        .namespace = "not-a-valid-namespace",
    };
    try std.testing.expectError(
        MemoryBackendConfig.ValidationError.InvalidNamespace,
        bad_ns.validate(),
    );
}
