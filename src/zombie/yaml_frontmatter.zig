// YAML frontmatter to JSON adapter.
//
// Wraps `zig-yaml` (kubkon/zig-yaml v0.2.0) and serializes the parsed tree
// to a JSON string. The downstream `config_parser.parseZombieConfig` and
// `config_markdown.parseSkillMetadata` continue to consume JSON; this file
// is the only seam that knows about YAML.

const std = @import("std");
const Allocator = std.mem.Allocator;
const yaml = @import("yaml");

pub const YamlError = error{ParseFailure};

/// Convert YAML frontmatter (the bytes between the `---` fences, already
/// extracted) to a single-line JSON object string. Caller owns the returned
/// slice. Empty input returns `"{}"`.
pub fn yamlFrontmatterToJson(alloc: Allocator, source: []const u8) (Allocator.Error || YamlError)![]u8 {
    var doc: yaml.Yaml = .{ .source = source };
    defer doc.deinit(alloc);

    doc.load(alloc) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => return error.ParseFailure,
    };

    var buf: std.ArrayList(u8) = .{};
    errdefer buf.deinit(alloc);
    const w = buf.writer(alloc);

    if (doc.docs.items.len == 0) {
        try w.writeAll("{}");
    } else {
        try writeJsonValue(w, doc.docs.items[0]);
    }

    return buf.toOwnedSlice(alloc);
}

fn writeJsonValue(w: anytype, v: yaml.Yaml.Value) !void {
    switch (v) {
        .empty => try w.writeAll("null"),
        .boolean => |b| try w.writeAll(if (b) "true" else "false"),
        .scalar => |s| try writeScalar(w, s),
        .list => |list| {
            try w.writeByte('[');
            for (list, 0..) |item, i| {
                if (i > 0) try w.writeAll(", ");
                try writeJsonValue(w, item);
            }
            try w.writeByte(']');
        },
        .map => |map| {
            try w.writeByte('{');
            var first = true;
            for (map.keys(), map.values()) |k, val| {
                if (!first) try w.writeAll(", ");
                try writeJsonString(w, k);
                try w.writeAll(": ");
                try writeJsonValue(w, val);
                first = false;
            }
            try w.writeByte('}');
        },
    }
}

// Known limitation (kubkon/zig-yaml v0.2.0): scalars arrive as raw bytes
// without quote-style metadata, so `name: true` (bool) and `name: "true"`
// (quoted string) are indistinguishable here. Both serialize as JSON `true`.
// Downstream schema validation rejects the type mismatch (e.g. parseNameField
// requires `.string`), so the user sees a config error rather than silent
// corruption — only the diagnostic specificity suffers. Documented here so
// future readers don't try to "fix" it without a parser-side hook.
fn writeScalar(w: anytype, s: []const u8) !void {
    if (std.mem.eql(u8, s, "true") or std.mem.eql(u8, s, "false")) {
        try w.writeAll(s);
        return;
    }
    if (std.mem.eql(u8, s, "null") or std.mem.eql(u8, s, "~")) {
        try w.writeAll("null");
        return;
    }
    if (isNumeric(s)) {
        try w.writeAll(s);
        return;
    }
    try writeJsonString(w, s);
}

fn writeJsonString(w: anytype, s: []const u8) !void {
    try w.writeByte('"');
    for (s) |c| {
        switch (c) {
            '"' => try w.writeAll("\\\""),
            '\\' => try w.writeAll("\\\\"),
            '\n' => try w.writeAll("\\n"),
            '\r' => try w.writeAll("\\r"),
            '\t' => try w.writeAll("\\t"),
            else => if (c < 0x20)
                try w.print("\\u{X:0>4}", .{c})
            else
                try w.writeByte(c),
        }
    }
    try w.writeByte('"');
}

fn isNumeric(v: []const u8) bool {
    if (v.len == 0) return false;
    var has_dot = false;
    for (v, 0..) |c, i| {
        if (c == '-' and i == 0) continue;
        if (c == '.' and !has_dot) {
            has_dot = true;
            continue;
        }
        if (!std.ascii.isDigit(c)) return false;
    }
    return true;
}

// ── Tests ──────────────────────────────────────────────────────────────────

test "yamlFrontmatterToJson: flat key-value" {
    const alloc = std.testing.allocator;
    const src = "name: lead-collector\ndaily_dollars: 5.0\nactive: true";
    const json = try yamlFrontmatterToJson(alloc, src);
    defer alloc.free(json);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, json, .{});
    defer parsed.deinit();
    const obj = parsed.value.object;
    try std.testing.expectEqualStrings("lead-collector", obj.get("name").?.string);
    try std.testing.expectApproxEqAbs(@as(f64, 5.0), obj.get("daily_dollars").?.float, 0.001);
    try std.testing.expect(obj.get("active").?.bool);
}

test "yamlFrontmatterToJson: nested object" {
    const alloc = std.testing.allocator;
    const src = "trigger:\n  type: webhook\n  source: agentmail";
    const json = try yamlFrontmatterToJson(alloc, src);
    defer alloc.free(json);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, json, .{});
    defer parsed.deinit();
    const trigger = parsed.value.object.get("trigger").?.object;
    try std.testing.expectEqualStrings("webhook", trigger.get("type").?.string);
    try std.testing.expectEqualStrings("agentmail", trigger.get("source").?.string);
}

test "yamlFrontmatterToJson: array items" {
    const alloc = std.testing.allocator;
    const src = "chain:\n  - lead-enricher\n  - crm-updater";
    const json = try yamlFrontmatterToJson(alloc, src);
    defer alloc.free(json);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, json, .{});
    defer parsed.deinit();
    const chain = parsed.value.object.get("chain").?.array;
    try std.testing.expectEqual(@as(usize, 2), chain.items.len);
    try std.testing.expectEqualStrings("lead-enricher", chain.items[0].string);
    try std.testing.expectEqualStrings("crm-updater", chain.items[1].string);
}

test "yamlFrontmatterToJson: inline array" {
    const alloc = std.testing.allocator;
    const src = "tags: [leads, email, agentmail]";
    const json = try yamlFrontmatterToJson(alloc, src);
    defer alloc.free(json);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, json, .{});
    defer parsed.deinit();
    const tags = parsed.value.object.get("tags").?.array;
    try std.testing.expectEqual(@as(usize, 3), tags.items.len);
    try std.testing.expectEqualStrings("leads", tags.items[0].string);
}

// Pins the kubkon/zig-yaml v0.2.0 limitation called out in writeScalar: a
// quoted magic-word scalar collapses to its bare-word JSON type. If this test
// breaks because the upstream parser starts surfacing quote style, update
// writeScalar to honor it and delete this pin.
test "yamlFrontmatterToJson: quoted magic-word scalars collapse to bare type (known limitation)" {
    const alloc = std.testing.allocator;
    const src =
        \\name: "true"
        \\version: "null"
    ;
    const json = try yamlFrontmatterToJson(alloc, src);
    defer alloc.free(json);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, json, .{});
    defer parsed.deinit();
    try std.testing.expect(parsed.value.object.get("name").? == .bool);
    try std.testing.expect(parsed.value.object.get("version").? == .null);
}

test "yamlFrontmatterToJson: two-level nesting via x-usezombie shape" {
    const alloc = std.testing.allocator;
    const src =
        \\name: foo
        \\x-usezombie:
        \\  network:
        \\    allow:
        \\      - api.fly.dev
        \\      - api.upstash.com
        \\  budget:
        \\    daily_dollars: 1.0
    ;
    const json = try yamlFrontmatterToJson(alloc, src);
    defer alloc.free(json);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, json, .{});
    defer parsed.deinit();
    const x = parsed.value.object.get("x-usezombie").?.object;
    const allow = x.get("network").?.object.get("allow").?.array;
    try std.testing.expectEqual(@as(usize, 2), allow.items.len);
    try std.testing.expectEqualStrings("api.fly.dev", allow.items[0].string);
    try std.testing.expectApproxEqAbs(
        @as(f64, 1.0),
        x.get("budget").?.object.get("daily_dollars").?.float,
        0.001,
    );
}
