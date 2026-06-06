//! In-run agent-memory store — non-durable SQLite `:memory:` plus capture/seed.
//!
//! Durable memory is the control plane's job (Postgres, written via the runner
//! push). The child holds only a NON-durable working store the agent recalls and
//! remembers against during one run: NullClaw's SQLite engine run file-less
//! (`db_path = ":memory:"`), so no on-disk memory file is ever created and no
//! credential/URL/DSN reaches the sandboxed agent. The store is seeded at run
//! start from memory the parent hydrated over the trusted plane and flushed back
//! out via `.memory` frames the parent forwards.
//!
//! Three pieces, all child-side:
//!   - `initRuntime` builds the `:memory:` runtime (a direct `BackendConfig`
//!     bypass — `registry.resolvePaths` hardcodes a workspace file path, so
//!     `:memory:` can only be reached by constructing the config by hand).
//!   - `seed` stores the hydrated entries into that runtime's store.
//!   - `MemoryCapturer` enumerates the store and writes a `.memory` frame the
//!     parent POSTs to the control plane (mid-run cadence + run end).

const std = @import("std");
const logging = @import("log");
const nullclaw = @import("nullclaw");
const clock = @import("common").clock;
const protocol = @import("contract").protocol;
const pipe_proto = @import("../pipe_proto.zig");

const memory_mod = nullclaw.memory;
const registry = memory_mod.registry;
const Memory = memory_mod.Memory;
const MemoryCategory = memory_mod.MemoryCategory;

const log = logging.scoped(.runner_inrun_memory);

const S_SQLITE = "sqlite";
const S_NONE = "none";
/// libsqlite opens a fresh per-connection database for this path — non-durable,
/// no file on disk, discarded when the connection closes at run end.
const DB_PATH_IN_MEMORY: [*:0]const u8 = ":memory:";

/// Build a NullClaw `MemoryRuntime` backed by a file-less SQLite database. The
/// store lives only for this child's lifetime; durability is the control plane.
/// Returns null on any backend error (sqlite disabled, open failure, OOM) — the
/// caller then runs with no recall (degrade, never block the run).
pub fn initRuntime(alloc: std.mem.Allocator, workspace_path: []const u8) ?memory_mod.MemoryRuntime {
    const desc = memory_mod.findBackend(S_SQLITE) orelse {
        log.warn("backend_disabled", .{ .backend = S_SQLITE });
        return null;
    };
    // Bypass resolvePaths (which would join a `workspace/memory.db` file path)
    // and hand the engine `:memory:` directly so no on-disk artifact is created.
    const backend_cfg = registry.BackendConfig{
        .db_path = DB_PATH_IN_MEMORY,
        .workspace_dir = workspace_path,
    };
    const instance = desc.create(alloc, backend_cfg) catch |err| {
        log.warn("inrun_store_init_failed", .{ .err = @errorName(err) });
        return null;
    };
    // `_db_path = null`: the `:memory:` literal is static, so deinit must not try
    // to free it. Minimal keyword-mode runtime — the agent drives the `Memory`
    // vtable directly (store/recall/list), never `MemoryRuntime.search`.
    return memory_mod.MemoryRuntime{
        .memory = instance.memory,
        .session_store = instance.session_store,
        .response_cache = null,
        .capabilities = desc.capabilities,
        .resolved = .{
            .primary_backend = S_SQLITE,
            .retrieval_mode = "keyword",
            .vector_mode = S_NONE,
            .embedding_provider = S_NONE,
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
        ._search_enabled = false,
    };
}

/// Seed the in-run store with the zombie's prior memory the parent hydrated.
/// Best-effort per entry: a single store failure is logged and skipped so a bad
/// row never aborts the run before the agent even starts. Content is never logged.
pub fn seed(mem: Memory, entries: []const protocol.MemoryDelta) void {
    var seeded: usize = 0;
    for (entries) |e| {
        mem.store(e.key, e.content, MemoryCategory.fromString(e.category), null) catch {
            log.warn("seed_entry_failed", .{ .key_len = e.key.len });
            continue;
        };
        seeded += 1;
    }
    if (entries.len > 0) log.info("memory_seeded", .{ .seeded = seeded, .offered = entries.len });
}

/// Flushes the in-run store out to the parent: enumerates every entry, drops
/// NullClaw-internal bootstrap/autosave keys, and writes the survivors as one
/// `.memory` frame on the progress fd. The parent forwards the frame to
/// `POST /v1/runners/me/memory/{zombie_id}`. Best-effort by contract — a capture
/// blip never fails the run (the durable record is the next checkpoint / run end).
pub const MemoryCapturer = struct {
    mem: Memory,
    /// Progress fd the `.memory` frame is written on (the child's stdout).
    fd: std.posix.fd_t,
    alloc: std.mem.Allocator,

    /// Enumerate → filter → serialize → write one `.memory` frame. Bounded by
    /// `MAX_MEMORY_PUSH_BYTES`: once the accumulated key+content+category bytes
    /// would exceed it, stop adding (the server truncates the same way), so a
    /// runaway store can't produce an oversized frame.
    pub fn capture(self: *const MemoryCapturer) void {
        var arena = std.heap.ArenaAllocator.init(self.alloc);
        defer arena.deinit();
        const a = arena.allocator();

        const entries = self.mem.list(a, null, null) catch |err| {
            log.warn("capture_list_failed", .{ .err = @errorName(err) });
            return;
        };

        var deltas: std.ArrayList(protocol.MemoryDelta) = .empty;
        var bytes: usize = 0;
        for (entries, 0..) |e, i| {
            if (memory_mod.isInternalMemoryEntryKeyOrContent(e.key, e.content)) continue;
            const cat = e.category.toString();
            bytes += e.key.len + e.content.len + cat.len;
            // Always keep at least the newest entry (i==0); past the budget, stop
            // and log — never a silent zero-count frame (mirrors the server window).
            if (i > 0 and bytes > protocol.MAX_MEMORY_PUSH_BYTES) {
                log.warn("capture_truncated", .{ .kept = deltas.items.len, .cap = protocol.MAX_MEMORY_PUSH_BYTES });
                break;
            }
            deltas.append(a, .{ .key = e.key, .content = e.content, .category = cat }) catch break;
        }

        const json = std.json.Stringify.valueAlloc(a, deltas.items, .{}) catch return;
        pipe_proto.writeFrame(self.fd, .memory, json) catch |err|
            log.warn("capture_frame_write_failed", .{ .err = @errorName(err) });
        // debug: a checkpoint can fire many times on a long run — keep it off info.
        log.debug("memory_captured_frame", .{ .count = deltas.items.len });
    }
};

// ── Tests ────────────────────────────────────────────────────────────────────

test "initRuntime builds a usable file-less store; seed + capture round-trip" {
    const alloc = std.testing.allocator;
    var rt = initRuntime(alloc, "/tmp") orelse return error.SkipZigTest; // sqlite disabled in some builds
    defer rt.deinit();

    // Seed two real entries plus an internal bootstrap key that must be filtered.
    seed(rt.memory, &.{
        .{ .key = "deploy_target", .content = "fly", .category = "core" },
        .{ .key = "owner", .content = "indy", .category = "core" },
    });
    rt.memory.store("__bootstrap.prompt.AGENTS.md", "noise", .core, null) catch {};

    const fds = try pipe_proto.osPipe();
    defer pipe_proto.osClose(fds[0]);
    var cap = MemoryCapturer{ .mem = rt.memory, .fd = fds[1], .alloc = alloc };
    cap.capture();
    pipe_proto.osClose(fds[1]);

    const dl = clock.nowMillis() + 5_000;
    const out = try pipe_proto.readFrame(alloc, fds[0], dl, 1 << 20);
    try std.testing.expect(out == .frame);
    defer alloc.free(out.frame.payload);
    try std.testing.expectEqual(pipe_proto.FrameType.memory, out.frame.ftype);

    const parsed = try std.json.parseFromSlice([]protocol.MemoryDelta, alloc, out.frame.payload, .{});
    defer parsed.deinit();
    // The two real entries survive; the bootstrap key is filtered out.
    try std.testing.expectEqual(@as(usize, 2), parsed.value.len);
}
