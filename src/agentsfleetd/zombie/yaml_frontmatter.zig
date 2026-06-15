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
const S_NULL = "null";
const S_FALSE = "false";
const S_TRUE = "true";

const S_PUNCT_FC763C = ", ";

pub fn yamlFrontmatterToJson(alloc: Allocator, source: []const u8) (Allocator.Error || YamlError)![]u8 {
    var doc: yaml.Yaml = .{ .source = source };
    defer doc.deinit(alloc);

    doc.load(alloc) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => return error.ParseFailure,
    };

    var aw: std.Io.Writer.Allocating = .init(alloc);
    errdefer aw.deinit();
    const w = &aw.writer;

    // The Allocating writer surfaces failure as Io.Writer's `WriteFailed`, but
    // its only real failure mode is OOM — map it back into the declared set.
    if (doc.docs.items.len == 0) {
        w.writeAll("{}") catch return error.OutOfMemory;
    } else {
        writeJsonValue(w, doc.docs.items[0]) catch return error.OutOfMemory;
    }

    return aw.toOwnedSlice();
}

fn writeJsonValue(w: anytype, v: yaml.Yaml.Value) !void {
    switch (v) {
        .empty => try w.writeAll(S_NULL),
        .boolean => |b| try w.writeAll(if (b) S_TRUE else S_FALSE),
        .scalar => |s| try writeScalar(w, s),
        .list => |list| {
            try w.writeByte('[');
            for (list, 0..) |item, i| {
                if (i > 0) try w.writeAll(S_PUNCT_FC763C);
                try writeJsonValue(w, item);
            }
            try w.writeByte(']');
        },
        .map => |map| {
            try w.writeByte('{');
            var first = true;
            for (map.keys(), map.values()) |k, val| {
                if (!first) try w.writeAll(S_PUNCT_FC763C);
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
    if (std.mem.eql(u8, s, S_TRUE) or std.mem.eql(u8, s, S_FALSE)) {
        try w.writeAll(s);
        return;
    }
    if (std.mem.eql(u8, s, S_NULL) or std.mem.eql(u8, s, "~")) {
        try w.writeAll(S_NULL);
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

// Returns true only when `v` is a JSON-valid number literal. Stricter than
// the YAML 1.2 numeric grammar on purpose: scalars that YAML accepts but
// JSON rejects (`1.`, `.5`, `-`, bare `.`, `01`) must be quoted, otherwise
// std.json.parseFromSlice fails downstream with a non-actionable error.
// Leading sign is accepted only as the first byte, decimal point at most
// once, must have a digit on each side of any decimal, and no leading
// zero on a multi-digit integer part (JSON spec).
fn isNumeric(v: []const u8) bool {
    if (v.len == 0) return false;
    var has_dot = false;
    var digit_seen = false;
    var int_part_len: usize = 0;
    var int_first: u8 = 0;
    for (v, 0..) |c, i| {
        if (c == '-' and i == 0) continue;
        if (c == '.' and !has_dot) {
            // Must have at least one integer digit before the dot.
            if (int_part_len == 0) return false;
            has_dot = true;
            continue;
        }
        if (!std.ascii.isDigit(c)) return false;
        digit_seen = true;
        if (!has_dot) {
            if (int_part_len == 0) int_first = c;
            int_part_len += 1;
        }
    }
    if (!digit_seen) return false;
    // No digit after the decimal point ("1." would emit invalid JSON).
    if (has_dot and v[v.len - 1] == '.') return false;
    // No leading zero on multi-digit integer part ("01" → invalid JSON).
    if (int_part_len > 1 and int_first == '0') return false;
    return true;
}

// ── Tests ──────────────────────────────────────────────────────────────────

test "yamlFrontmatterToJson: flat key-value" {
    const alloc = std.testing.allocator;
    const src = "name: platform-ops\ndaily_dollars: 5.0\nactive: true";
    const json = try yamlFrontmatterToJson(alloc, src);
    defer alloc.free(json);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, json, .{});
    defer parsed.deinit();
    const obj = parsed.value.object;
    try std.testing.expectEqualStrings("platform-ops", obj.get("name").?.string);
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
    const src = "credentials:\n  - agentmail_api_key\n  - openai_api_key";
    const json = try yamlFrontmatterToJson(alloc, src);
    defer alloc.free(json);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, json, .{});
    defer parsed.deinit();
    const creds = parsed.value.object.get("credentials").?.array;
    try std.testing.expectEqual(@as(usize, 2), creds.items.len);
    try std.testing.expectEqualStrings("agentmail_api_key", creds.items[0].string);
    try std.testing.expectEqualStrings("openai_api_key", creds.items[1].string);
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

// Pins isNumeric's JSON-correctness guard: scalars that YAML accepts but
// JSON rejects must be quoted, not emitted as bare numbers. Each input
// here, if isNumeric returned true, would emit invalid JSON downstream.
test "yamlFrontmatterToJson: invalid JSON-numeric scalars get quoted (isNumeric guard)" {
    const alloc = std.testing.allocator;
    const cases = [_][]const u8{
        "name: -", // bare sign
        "name: .", // sole decimal
        "name: 1.", // trailing decimal
        "name: .5", // leading decimal
        "name: '01'", // leading zero — already quoted but proves quoted path works
        "name: 01", // leading zero unquoted — must NOT emit bare 01 (JSON rejects)
    };
    for (cases) |src| {
        const json = try yamlFrontmatterToJson(alloc, src);
        defer alloc.free(json);
        // Must round-trip through std.json without error: the guard kicked in
        // and produced a quoted string instead of an invalid bare number.
        const parsed = try std.json.parseFromSlice(std.json.Value, alloc, json, .{});
        defer parsed.deinit();
        const v = parsed.value.object.get("name").?;
        try std.testing.expect(v == .string);
    }
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

test "yamlFrontmatterToJson: two-level nesting via x-agentsfleet shape" {
    const alloc = std.testing.allocator;
    const src =
        \\name: foo
        \\x-agentsfleet:
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
    const x = parsed.value.object.get("x-agentsfleet").?.object;
    const allow = x.get("network").?.object.get("allow").?.array;
    try std.testing.expectEqual(@as(usize, 2), allow.items.len);
    try std.testing.expectEqualStrings("api.fly.dev", allow.items[0].string);
    try std.testing.expectApproxEqAbs(
        @as(f64, 1.0),
        x.get("budget").?.object.get("daily_dollars").?.float,
        0.001,
    );
}
