// YAML frontmatter to JSON converter for TRIGGER.md parsing.
//
// Supports the subset of YAML used in TRIGGER.md frontmatter:
//   - Top-level scalar key: value
//   - One level of nested key: value
//   - Arrays via "- item" syntax
//   - Inline arrays via [a, b, c] syntax
//   - Booleans (true/false) and numbers pass through unquoted

const std = @import("std");
const Allocator = std.mem.Allocator;

const KeyValue = struct { key: []const u8, value: []const u8 };

// yamlFrontmatterToJson converts simple YAML key-value pairs to a JSON string.
// Supports: top-level scalars, one level of nesting, and arrays (- items).
// YamlToJsonConverter holds state for the line-by-line YAML→JSON conversion.
const YamlToJsonConverter = struct {
    first_top: bool = true,
    in_nested: bool = false,
    first_nested: bool = true,
    in_array: bool = false,
    first_array: bool = true,
    pending_bracket: bool = false,

    fn processLine(self: *YamlToJsonConverter, w: anytype, line: []const u8) !void {
        if (isArrayItem(line)) {
            try self.handleArrayItem(w, line);
            return;
        }
        if (self.in_array) {
            try w.writeByte(']');
            self.in_array = false;
        }
        if (isIndented(line) and (self.in_nested or self.pending_bracket)) {
            try self.handleNestedKey(w, line);
            return;
        }
        try self.closeNested(w);
        try self.handleTopLevelKey(w, line);
    }

    fn handleArrayItem(self: *YamlToJsonConverter, w: anytype, line: []const u8) !void {
        if (self.pending_bracket) {
            try w.writeByte('[');
            self.pending_bracket = false;
            self.in_nested = false;
            self.in_array = true;
            self.first_array = true;
        }
        if (!self.in_array) {
            self.in_array = true;
            self.first_array = true;
        }
        if (!self.first_array) try w.writeAll(", ");
        try writeJsonString(w, extractArrayValue(line));
        self.first_array = false;
    }

    fn handleNestedKey(self: *YamlToJsonConverter, w: anytype, line: []const u8) !void {
        if (self.pending_bracket) {
            try w.writeByte('{');
            self.pending_bracket = false;
            self.in_nested = true;
            self.first_nested = true;
        }
        if (extractKeyValue(line)) |kv| {
            if (!self.first_nested) try w.writeAll(", ");
            try writeJsonString(w, kv.key);
            try w.writeAll(": ");
            if (kv.value.len == 0) {
                try w.writeByte('[');
                self.in_array = true;
                self.first_array = true;
            } else {
                try writeJsonValue(w, kv.value);
            }
            self.first_nested = false;
        }
    }

    fn closeNested(self: *YamlToJsonConverter, w: anytype) !void {
        if (self.in_nested) {
            try w.writeByte('}');
            self.in_nested = false;
        }
        if (self.pending_bracket) {
            try w.writeAll("{}");
            self.pending_bracket = false;
        }
    }

    fn handleTopLevelKey(self: *YamlToJsonConverter, w: anytype, line: []const u8) !void {
        if (extractKeyValue(line)) |kv| {
            if (!self.first_top) try w.writeAll(", ");
            try writeJsonString(w, kv.key);
            try w.writeAll(": ");
            if (kv.value.len == 0) {
                self.pending_bracket = true;
            } else {
                try writeJsonValue(w, kv.value);
            }
            self.first_top = false;
        }
    }

    fn finish(self: *YamlToJsonConverter, w: anytype) !void {
        if (self.in_array) try w.writeByte(']');
        if (self.in_nested) try w.writeByte('}');
        if (self.pending_bracket) try w.writeAll("{}");
        try w.writeByte('}');
    }
};

pub fn yamlFrontmatterToJson(alloc: Allocator, yaml: []const u8) ![]u8 {
    var buf: std.ArrayList(u8) = .{};
    errdefer buf.deinit(alloc);
    const w = buf.writer(alloc);
    try w.writeByte('{');

    var conv = YamlToJsonConverter{};
    var lines = std.mem.splitScalar(u8, yaml, '\n');
    while (lines.next()) |raw_line| {
        const line = std.mem.trimRight(u8, raw_line, " \t\r");
        if (line.len == 0) continue;
        if (std.mem.startsWith(u8, std.mem.trimLeft(u8, line, " \t"), "#")) continue;
        try conv.processLine(w, line);
    }
    try conv.finish(w);

    return buf.toOwnedSlice(alloc);
}

fn extractKeyValue(line: []const u8) ?KeyValue {
    const trimmed = std.mem.trimLeft(u8, line, " \t");
    const colon = std.mem.indexOfScalar(u8, trimmed, ':') orelse return null;
    if (colon == 0) return null;
    const key = trimmed[0..colon];
    const rest = if (colon + 1 < trimmed.len) std.mem.trimLeft(u8, trimmed[colon + 1 ..], " \t") else "";
    return .{ .key = key, .value = rest };
}

fn isIndented(line: []const u8) bool {
    return line.len > 0 and (line[0] == ' ' or line[0] == '\t');
}

fn isArrayItem(line: []const u8) bool {
    const trimmed = std.mem.trimLeft(u8, line, " \t");
    return std.mem.startsWith(u8, trimmed, "- ");
}

fn extractArrayValue(line: []const u8) []const u8 {
    const trimmed = std.mem.trimLeft(u8, line, " \t");
    return std.mem.trim(u8, trimmed[2..], " \t");
}

fn writeJsonString(w: anytype, s: []const u8) !void {
    try w.writeByte('"');
    for (s) |c| {
        if (c == '"') {
            try w.writeAll("\\\"");
        } else if (c == '\\') {
            try w.writeAll("\\\\");
        } else {
            try w.writeByte(c);
        }
    }
    try w.writeByte('"');
}

fn writeJsonValue(w: anytype, v: []const u8) !void {
    // Check for inline array: [a, b, c]
    if (v.len >= 2 and v[0] == '[' and v[v.len - 1] == ']') {
        const inner = std.mem.trim(u8, v[1 .. v.len - 1], " \t");
        try w.writeByte('[');
        var parts = std.mem.splitScalar(u8, inner, ',');
        var first = true;
        while (parts.next()) |part| {
            const trimmed = std.mem.trim(u8, part, " \t");
            if (trimmed.len == 0) continue;
            if (!first) try w.writeAll(", ");
            try writeJsonString(w, trimmed);
            first = false;
        }
        try w.writeByte(']');
        return;
    }
    // Boolean/number pass-through
    if (std.mem.eql(u8, v, "true") or std.mem.eql(u8, v, "false")) {
        try w.writeAll(v);
        return;
    }
    // Number
    if (isNumeric(v)) {
        try w.writeAll(v);
        return;
    }
    try writeJsonString(w, v);
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

// ── Tests ──────────────────────────────────────────────────────────────

test "yamlFrontmatterToJson: flat key-value" {
    const alloc = std.testing.allocator;
    const yaml = "name: lead-collector\ndaily_dollars: 5.0\nactive: true";
    const json = try yamlFrontmatterToJson(alloc, yaml);
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
    const yaml = "trigger:\n  type: webhook\n  source: agentmail";
    const json = try yamlFrontmatterToJson(alloc, yaml);
    defer alloc.free(json);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, json, .{});
    defer parsed.deinit();
    const trigger = parsed.value.object.get("trigger").?.object;
    try std.testing.expectEqualStrings("webhook", trigger.get("type").?.string);
    try std.testing.expectEqualStrings("agentmail", trigger.get("source").?.string);
}

test "yamlFrontmatterToJson: array items" {
    const alloc = std.testing.allocator;
    const yaml = "chain:\n  - lead-enricher\n  - crm-updater";
    const json = try yamlFrontmatterToJson(alloc, yaml);
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
    const yaml = "tags: [leads, email, agentmail]";
    const json = try yamlFrontmatterToJson(alloc, yaml);
    defer alloc.free(json);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, json, .{});
    defer parsed.deinit();
    const tags = parsed.value.object.get("tags").?.array;
    try std.testing.expectEqual(@as(usize, 3), tags.items.len);
    try std.testing.expectEqualStrings("leads", tags.items[0].string);
}
