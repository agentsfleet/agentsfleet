// Tests for zombie/event_loop.zig — split out to keep event_loop.zig under 500 lines.
//
// Spec dimensions covered:
//   3.1-3.4 are integration tests (DB+Redis+Executor) — see test-integration suite.
//   Unit tests here cover: memory safety, JSON escaping, struct contracts, defaults.

const std = @import("std");
const event_loop = @import("event_loop.zig");
const zombie_config = @import("config.zig");

const ZombieSession = event_loop.ZombieSession;
const EventResult = event_loop.EventResult;
const EventLoopConfig = event_loop.EventLoopConfig;

// ── T1: Happy path ──────────────────────────────────────────────────────

test "T1: ZombieSession.deinit frees all owned memory" {
    const alloc = std.testing.allocator;

    const config_json =
        \\{"name":"test","trigger":{"type":"webhook","source":"email"},"tools":["agentmail"],"budget":{"daily_dollars":5.0}}
    ;
    const config = try zombie_config.parseZombieConfig(alloc, config_json);

    const source_md = try alloc.dupe(u8, "---\nname: test\n---\nYou are a test agent.");
    const instructions = zombie_config.extractZombieInstructions(source_md);

    var session = ZombieSession{
        .zombie_id = try alloc.dupe(u8, "019abc"),
        .workspace_id = try alloc.dupe(u8, "ws-001"),
        .config = config,
        .instructions = instructions,
        .context_json = try alloc.dupe(u8, "{}"),
        .source_markdown = source_md,
    };
    session.deinit(alloc);
    // std.testing.allocator leak detector verifies no leaks
}

test "T1: EventResult.deinit frees agent_response" {
    const alloc = std.testing.allocator;
    const result = EventResult{
        .status = .processed,
        .agent_response = try alloc.dupe(u8, "I replied to the lead."),
        .token_count = 500,
        .wall_seconds = 3,
    };
    result.deinit(alloc);
}

test "T1: updateSessionContext produces valid JSON" {
    const alloc = std.testing.allocator;

    const config_json =
        \\{"name":"test","trigger":{"type":"webhook","source":"email"},"tools":["agentmail"],"budget":{"daily_dollars":5.0}}
    ;
    const config = try zombie_config.parseZombieConfig(alloc, config_json);
    const source_md = try alloc.dupe(u8, "---\nname: test\n---\nYou are a test agent.");

    var session = ZombieSession{
        .zombie_id = try alloc.dupe(u8, "z1"),
        .workspace_id = try alloc.dupe(u8, "ws1"),
        .config = config,
        .instructions = zombie_config.extractZombieInstructions(source_md),
        .context_json = try alloc.dupe(u8, "{}"),
        .source_markdown = source_md,
    };
    defer session.deinit(alloc);

    try event_loop.updateSessionContext(alloc, &session, "evt_001", "Hello, I processed the lead.");

    // Result must be valid JSON
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, session.context_json, .{});
    defer parsed.deinit();

    // Must contain the event_id
    const last_event = parsed.value.object.get("last_event_id") orelse return error.MissingField;
    try std.testing.expectEqualStrings("evt_001", last_event.string);
}

// ── T2: Edge cases ──────────────────────────────────────────────────────

test "T2: truncateForJson returns full string when under limit" {
    const s = "short string";
    try std.testing.expectEqualStrings(s, event_loop.truncateForJson(s));
}

test "T2: truncateForJson truncates at 2048 bytes" {
    var buf: [4096]u8 = undefined;
    @memset(&buf, 'x');
    const result = event_loop.truncateForJson(&buf);
    try std.testing.expectEqual(@as(usize, 2048), result.len);
}

test "T2: truncateForJson handles empty string" {
    try std.testing.expectEqualStrings("", event_loop.truncateForJson(""));
}

test "T2: truncateForJson handles exactly 2048 bytes" {
    var buf: [2048]u8 = undefined;
    @memset(&buf, 'a');
    const result = event_loop.truncateForJson(&buf);
    try std.testing.expectEqual(@as(usize, 2048), result.len);
}

test "T2: truncateForJson handles 2049 bytes (one over)" {
    var buf: [2049]u8 = undefined;
    @memset(&buf, 'b');
    const result = event_loop.truncateForJson(&buf);
    try std.testing.expectEqual(@as(usize, 2048), result.len);
}

test "T2: truncateForJson does not split multi-byte UTF-8 codepoint" {
    // Build a buffer: 2046 ASCII bytes + one 3-byte UTF-8 char (e.g. U+3042 'あ' = E3 81 82).
    // Total = 2049. Naive slice at 2048 would cut between bytes 2 and 3 of the codepoint,
    // producing invalid UTF-8. The fix must walk back to 2046.
    var buf: [2049]u8 = undefined;
    @memset(&buf, 'a');
    buf[2046] = 0xE3; // 'あ' byte 1
    buf[2047] = 0x81; // 'あ' byte 2
    buf[2048] = 0x82; // 'あ' byte 3
    const result = event_loop.truncateForJson(&buf);
    // Must stop before the 3-byte char — at 2046, not 2048 (mid-sequence).
    try std.testing.expectEqual(@as(usize, 2046), result.len);
    try std.testing.expect(std.unicode.utf8ValidateSlice(result));
}

test "T2: truncateForJson preserves complete multi-byte char before boundary" {
    // 2045 ASCII + one 3-byte char at [2045..2048] = exactly 2048 bytes total.
    var buf: [2048]u8 = undefined;
    @memset(&buf, 'a');
    buf[2045] = 0xE3;
    buf[2046] = 0x81;
    buf[2047] = 0x82;
    const result = event_loop.truncateForJson(&buf);
    // The 3-byte char fits entirely within 2048 — no truncation needed.
    try std.testing.expectEqual(@as(usize, 2048), result.len);
    try std.testing.expect(std.unicode.utf8ValidateSlice(result));
}

// ── T3: Error paths / JSON injection regression ─────────────────────────

test "T3: updateSessionContext escapes double quotes in agent_response" {
    const alloc = std.testing.allocator;

    const config_json =
        \\{"name":"test","trigger":{"type":"webhook","source":"email"},"tools":["agentmail"],"budget":{"daily_dollars":5.0}}
    ;
    const config = try zombie_config.parseZombieConfig(alloc, config_json);
    const source_md = try alloc.dupe(u8, "---\nname: test\n---\nAgent.");

    var session = ZombieSession{
        .zombie_id = try alloc.dupe(u8, "z1"),
        .workspace_id = try alloc.dupe(u8, "ws1"),
        .config = config,
        .instructions = zombie_config.extractZombieInstructions(source_md),
        .context_json = try alloc.dupe(u8, "{}"),
        .source_markdown = source_md,
    };
    defer session.deinit(alloc);

    // Agent response contains double quotes — must be escaped in JSON
    try event_loop.updateSessionContext(alloc, &session, "evt_002", "He said \"hello\" to the lead.");

    // Must still parse as valid JSON
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, session.context_json, .{});
    defer parsed.deinit();

    const resp = parsed.value.object.get("last_response") orelse return error.MissingField;
    try std.testing.expect(std.mem.indexOf(u8, resp.string, "hello") != null);
}

test "T3: updateSessionContext escapes backslashes in event_id" {
    const alloc = std.testing.allocator;

    const config_json =
        \\{"name":"test","trigger":{"type":"webhook","source":"email"},"tools":["agentmail"],"budget":{"daily_dollars":5.0}}
    ;
    const config = try zombie_config.parseZombieConfig(alloc, config_json);
    const source_md = try alloc.dupe(u8, "---\nname: test\n---\nAgent.");

    var session = ZombieSession{
        .zombie_id = try alloc.dupe(u8, "z1"),
        .workspace_id = try alloc.dupe(u8, "ws1"),
        .config = config,
        .instructions = zombie_config.extractZombieInstructions(source_md),
        .context_json = try alloc.dupe(u8, "{}"),
        .source_markdown = source_md,
    };
    defer session.deinit(alloc);

    // Event ID with backslashes — must not corrupt JSON
    try event_loop.updateSessionContext(alloc, &session, "evt\\path\\id", "ok");

    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, session.context_json, .{});
    defer parsed.deinit();

    const eid = parsed.value.object.get("last_event_id") orelse return error.MissingField;
    try std.testing.expectEqualStrings("evt\\path\\id", eid.string);
}

test "T3: updateSessionContext escapes newlines and control chars" {
    const alloc = std.testing.allocator;

    const config_json =
        \\{"name":"test","trigger":{"type":"webhook","source":"email"},"tools":["agentmail"],"budget":{"daily_dollars":5.0}}
    ;
    const config = try zombie_config.parseZombieConfig(alloc, config_json);
    const source_md = try alloc.dupe(u8, "---\nname: test\n---\nAgent.");

    var session = ZombieSession{
        .zombie_id = try alloc.dupe(u8, "z1"),
        .workspace_id = try alloc.dupe(u8, "ws1"),
        .config = config,
        .instructions = zombie_config.extractZombieInstructions(source_md),
        .context_json = try alloc.dupe(u8, "{}"),
        .source_markdown = source_md,
    };
    defer session.deinit(alloc);

    // Response with embedded newlines and tabs
    try event_loop.updateSessionContext(alloc, &session, "evt_003", "line1\nline2\ttab");

    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, session.context_json, .{});
    defer parsed.deinit();

    const resp = parsed.value.object.get("last_response") orelse return error.MissingField;
    try std.testing.expect(std.mem.indexOf(u8, resp.string, "line1") != null);
    try std.testing.expect(std.mem.indexOf(u8, resp.string, "line2") != null);
}

test "T3: updateSessionContext with empty event_id and response" {
    const alloc = std.testing.allocator;

    const config_json =
        \\{"name":"test","trigger":{"type":"webhook","source":"email"},"tools":["agentmail"],"budget":{"daily_dollars":5.0}}
    ;
    const config = try zombie_config.parseZombieConfig(alloc, config_json);
    const source_md = try alloc.dupe(u8, "---\nname: test\n---\nAgent.");

    var session = ZombieSession{
        .zombie_id = try alloc.dupe(u8, "z1"),
        .workspace_id = try alloc.dupe(u8, "ws1"),
        .config = config,
        .instructions = zombie_config.extractZombieInstructions(source_md),
        .context_json = try alloc.dupe(u8, "{}"),
        .source_markdown = source_md,
    };
    defer session.deinit(alloc);

    try event_loop.updateSessionContext(alloc, &session, "", "");

    // Must still be valid JSON
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, session.context_json, .{});
    defer parsed.deinit();

    const eid = parsed.value.object.get("last_event_id") orelse return error.MissingField;
    try std.testing.expectEqualStrings("", eid.string);
}

test "T3: updateSessionContext replaces previous context (no leak)" {
    const alloc = std.testing.allocator;

    const config_json =
        \\{"name":"test","trigger":{"type":"webhook","source":"email"},"tools":["agentmail"],"budget":{"daily_dollars":5.0}}
    ;
    const config = try zombie_config.parseZombieConfig(alloc, config_json);
    const source_md = try alloc.dupe(u8, "---\nname: test\n---\nAgent.");

    var session = ZombieSession{
        .zombie_id = try alloc.dupe(u8, "z1"),
        .workspace_id = try alloc.dupe(u8, "ws1"),
        .config = config,
        .instructions = zombie_config.extractZombieInstructions(source_md),
        .context_json = try alloc.dupe(u8, "{}"),
        .source_markdown = source_md,
    };
    defer session.deinit(alloc);

    // First update
    try event_loop.updateSessionContext(alloc, &session, "evt_001", "first");
    // Second update — must free the first context (leak detector catches if not)
    try event_loop.updateSessionContext(alloc, &session, "evt_002", "second");

    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, session.context_json, .{});
    defer parsed.deinit();

    const eid = parsed.value.object.get("last_event_id") orelse return error.MissingField;
    try std.testing.expectEqualStrings("evt_002", eid.string);
}

// ── T7: Regression — struct contracts ───────────────────────────────────

test "T7: ZombieSession struct has all required fields" {
    const info = @typeInfo(ZombieSession).@"struct";
    comptime {
        var found_zombie_id = false;
        var found_workspace_id = false;
        var found_config = false;
        var found_instructions = false;
        var found_context_json = false;
        var found_source_markdown = false;
        for (info.fields) |f| {
            if (std.mem.eql(u8, f.name, "zombie_id")) found_zombie_id = true;
            if (std.mem.eql(u8, f.name, "workspace_id")) found_workspace_id = true;
            if (std.mem.eql(u8, f.name, "config")) found_config = true;
            if (std.mem.eql(u8, f.name, "instructions")) found_instructions = true;
            if (std.mem.eql(u8, f.name, "context_json")) found_context_json = true;
            if (std.mem.eql(u8, f.name, "source_markdown")) found_source_markdown = true;
        }
        if (!found_zombie_id) @compileError("ZombieSession missing 'zombie_id'");
        if (!found_workspace_id) @compileError("ZombieSession missing 'workspace_id'");
        if (!found_config) @compileError("ZombieSession missing 'config'");
        if (!found_instructions) @compileError("ZombieSession missing 'instructions'");
        if (!found_context_json) @compileError("ZombieSession missing 'context_json'");
        if (!found_source_markdown) @compileError("ZombieSession missing 'source_markdown'");
    }
}

test "T7: EventResult.Status enum has exactly 3 variants" {
    const fields = @typeInfo(EventResult.Status).@"enum".fields;
    try std.testing.expectEqual(@as(usize, 3), fields.len);
}

test "T7: EventLoopConfig has sensible defaults" {
    const cfg = EventLoopConfig{
        .pool = undefined,
        .redis = undefined,
        .executor = undefined,
        .running = undefined,
    };
    try std.testing.expectEqual(@as(u64, 2_000), cfg.poll_interval_ms);
    try std.testing.expectEqualStrings("/tmp/zombie", cfg.workspace_path);
}

// ── T10: Constants — zombie error codes ─────────────────────────────────

const error_codes = @import("../errors/error_registry.zig");

test "T10: UZ-ZMB error codes follow naming convention" {
    try std.testing.expect(std.mem.startsWith(u8, error_codes.ERR_ZOMBIE_BUDGET_EXCEEDED, "UZ-ZMB-"));
    try std.testing.expect(std.mem.startsWith(u8, error_codes.ERR_ZOMBIE_AGENT_TIMEOUT, "UZ-ZMB-"));
    try std.testing.expect(std.mem.startsWith(u8, error_codes.ERR_ZOMBIE_CREDENTIAL_MISSING, "UZ-ZMB-"));
    try std.testing.expect(std.mem.startsWith(u8, error_codes.ERR_ZOMBIE_CLAIM_FAILED, "UZ-ZMB-"));
    try std.testing.expect(std.mem.startsWith(u8, error_codes.ERR_ZOMBIE_CHECKPOINT_FAILED, "UZ-ZMB-"));
}

test "T10: UZ-ZMB error codes are distinct" {
    const codes = [_][]const u8{
        error_codes.ERR_ZOMBIE_BUDGET_EXCEEDED,
        error_codes.ERR_ZOMBIE_AGENT_TIMEOUT,
        error_codes.ERR_ZOMBIE_CREDENTIAL_MISSING,
        error_codes.ERR_ZOMBIE_CLAIM_FAILED,
        error_codes.ERR_ZOMBIE_CHECKPOINT_FAILED,
    };
    // All pairs must be distinct
    for (codes, 0..) |a, i| {
        for (codes[i + 1 ..]) |b| {
            try std.testing.expect(!std.mem.eql(u8, a, b));
        }
    }
}

test "T10: all UZ-ZMB error codes have hints" {
    try std.testing.expect(error_codes.hint(error_codes.ERR_ZOMBIE_BUDGET_EXCEEDED).len > 0);
    try std.testing.expect(error_codes.hint(error_codes.ERR_ZOMBIE_AGENT_TIMEOUT).len > 0);
    try std.testing.expect(error_codes.hint(error_codes.ERR_ZOMBIE_CREDENTIAL_MISSING).len > 0);
    try std.testing.expect(error_codes.hint(error_codes.ERR_ZOMBIE_CLAIM_FAILED).len > 0);
    try std.testing.expect(error_codes.hint(error_codes.ERR_ZOMBIE_CHECKPOINT_FAILED).len > 0);
}

test "T10: UZ-ZMB hints are actionable with CLI commands" {
    // Budget hint must suggest zombiectl command
    const budget_hint = error_codes.hint(error_codes.ERR_ZOMBIE_BUDGET_EXCEEDED);
    try std.testing.expect(std.mem.indexOf(u8, budget_hint, "zombiectl") != null);

    // Credential hint must suggest zombiectl credential add
    const cred_hint = error_codes.hint(error_codes.ERR_ZOMBIE_CREDENTIAL_MISSING);
    try std.testing.expect(std.mem.indexOf(u8, cred_hint, "credential add") != null);

    // Claim hint must mention zombie_id and status
    const claim_hint = error_codes.hint(error_codes.ERR_ZOMBIE_CLAIM_FAILED);
    try std.testing.expect(std.mem.indexOf(u8, claim_hint, "zombie_id") != null);
}

test "T10: UZ-ZMB hints do not leak secrets" {
    const codes = [_][]const u8{
        error_codes.ERR_ZOMBIE_BUDGET_EXCEEDED,
        error_codes.ERR_ZOMBIE_AGENT_TIMEOUT,
        error_codes.ERR_ZOMBIE_CREDENTIAL_MISSING,
        error_codes.ERR_ZOMBIE_CLAIM_FAILED,
        error_codes.ERR_ZOMBIE_CHECKPOINT_FAILED,
    };
    for (codes) |code| {
        const h = error_codes.hint(code);
        // Must not contain credential prefixes or raw tokens
        try std.testing.expect(std.mem.indexOf(u8, h, "sk-ant-") == null);
        try std.testing.expect(std.mem.indexOf(u8, h, "Bearer ") == null);
        try std.testing.expect(std.mem.indexOf(u8, h, "op://") == null);
    }
}

// ── T10: Constants — zombie stream constants ────────────────────────────

const queue_consts = @import("../queue/constants.zig");

test "T10: zombie stream prefix/suffix build valid key format" {
    var buf: [128]u8 = undefined;
    const key = try std.fmt.bufPrint(&buf, "{s}{s}{s}", .{
        queue_consts.zombie_stream_prefix,
        "019abc-def",
        queue_consts.zombie_stream_suffix,
    });
    try std.testing.expectEqualStrings("zombie:019abc-def:events", key);
}

test "T10: zombie consumer group is non-empty" {
    try std.testing.expect(queue_consts.zombie_consumer_group.len > 0);
}

test "T10: zombie xread block time matches pipeline pattern" {
    // Both pipeline and zombie use 5000ms block
    try std.testing.expectEqualStrings("5000", queue_consts.zombie_xread_block_ms);
    try std.testing.expectEqualStrings("5000", queue_consts.xread_block_ms);
}

test "T10: zombie reclaim interval matches pipeline pattern" {
    try std.testing.expectEqual(queue_consts.reclaim_interval_ms, queue_consts.zombie_reclaim_interval_ms);
}

// ── T11: Memory — ZombieEvent.deinit ────────────────────────────────────

const redis_zombie = @import("../queue/redis_zombie.zig");

test "T11: ZombieEvent.deinit frees all owned fields" {
    const alloc = std.testing.allocator;
    var evt = redis_zombie.ZombieEvent{
        .message_id = try alloc.dupe(u8, "1234567890-0"),
        .event_id = try alloc.dupe(u8, "evt_abc123"),
        .event_type = try alloc.dupe(u8, "message.received"),
        .source = try alloc.dupe(u8, "agentmail"),
        .data_json = try alloc.dupe(u8, "{\"from\":\"user@example.com\"}"),
    };
    evt.deinit(alloc);
    // leak detector verifies no leaks
}

test "T11: ZombieEvent.deinit handles empty fields" {
    const alloc = std.testing.allocator;
    var evt = redis_zombie.ZombieEvent{
        .message_id = try alloc.dupe(u8, "0-0"),
        .event_id = try alloc.dupe(u8, ""),
        .event_type = try alloc.dupe(u8, ""),
        .source = try alloc.dupe(u8, ""),
        .data_json = try alloc.dupe(u8, "{}"),
    };
    evt.deinit(alloc);
}
