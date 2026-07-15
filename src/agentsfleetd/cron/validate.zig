//! Allocation-free guards for schedule input.

const std = @import("std");

pub const MAX_CRON_LEN: usize = 128;
pub const MAX_TIMEZONE_LEN: usize = 64;
pub const MAX_MESSAGE_LEN: usize = 8192;

pub const ValidationError = error{
    InvalidCron,
    InvalidTimezone,
    InvalidMessage,
};

const Bounds = struct { min: u8, max: u8 };
const FIELD_BOUNDS = [_]Bounds{
    .{ .min = 0, .max = 59 },
    .{ .min = 0, .max = 23 },
    .{ .min = 1, .max = 31 },
    .{ .min = 1, .max = 12 },
    .{ .min = 0, .max = 7 },
};

pub fn cron(expression: []const u8) ValidationError!void {
    if (expression.len == 0 or expression.len > MAX_CRON_LEN) return error.InvalidCron;
    var fields = std.mem.tokenizeAny(u8, expression, " \t\r\n");
    for (FIELD_BOUNDS) |bounds| {
        const field = fields.next() orelse return error.InvalidCron;
        if (!validField(field, bounds)) return error.InvalidCron;
    }
    if (fields.next() != null) return error.InvalidCron;
}

pub fn timezone(value: []const u8) ValidationError!void {
    if (value.len == 0 or value.len > MAX_TIMEZONE_LEN) return error.InvalidTimezone;
    var previous_slash = false;
    for (value, 0..) |char, index| {
        const slash = char == '/';
        if (slash and (index == 0 or previous_slash or index + 1 == value.len)) {
            return error.InvalidTimezone;
        }
        if (!slash and !std.ascii.isAlphanumeric(char) and char != '_' and char != '-' and char != '+') {
            return error.InvalidTimezone;
        }
        previous_slash = slash;
    }
}

pub fn message(value: []const u8) ValidationError!void {
    if (value.len == 0 or value.len > MAX_MESSAGE_LEN) return error.InvalidMessage;
    for (value) |char| if (!std.ascii.isWhitespace(char)) return;
    return error.InvalidMessage;
}

fn validField(field: []const u8, bounds: Bounds) bool {
    if (field.len == 0) return false;
    var items = std.mem.splitScalar(u8, field, ',');
    while (items.next()) |item| if (!validItem(item, bounds)) return false;
    return true;
}

fn validItem(item: []const u8, bounds: Bounds) bool {
    if (item.len == 0) return false;
    var step_parts = std.mem.splitScalar(u8, item, '/');
    const base = step_parts.next() orelse return false;
    if (step_parts.next()) |step_raw| {
        if (step_parts.next() != null) return false;
        const step = parseNumber(step_raw) orelse return false;
        const span = @as(u16, bounds.max) - bounds.min + 1;
        if (step == 0 or step > span) return false;
    }
    if (std.mem.eql(u8, base, "*")) return true;

    var range = std.mem.splitScalar(u8, base, '-');
    const start = parseNumber(range.next() orelse return false) orelse return false;
    const end_raw = range.next() orelse return inBounds(start, bounds);
    if (range.next() != null) return false;
    const end = parseNumber(end_raw) orelse return false;
    return inBounds(start, bounds) and inBounds(end, bounds) and start <= end;
}

fn parseNumber(value: []const u8) ?u16 {
    if (value.len == 0) return null;
    return std.fmt.parseUnsigned(u16, value, 10) catch null;
}

fn inBounds(value: u16, bounds: Bounds) bool {
    return value >= bounds.min and value <= bounds.max;
}
