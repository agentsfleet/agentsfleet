//! Query filter + cursor + `since=` + glob helpers for the
//! `core.zombie_events` read endpoints. Lives beside the store so the
//! store stays under the file length cap and the parsing logic is
//! independently testable.

const std = @import("std");
const base64 = std.base64.url_safe_no_pad;

pub const Filter = struct {
    /// Page size. Caller clamps to its own min/max before calling.
    limit: u32,
    /// Opaque base64url cursor or null for the first page.
    cursor: ?[]const u8 = null,
    /// Glob-style actor filter (`steer:*`, `webhook:*`, `webhook:github`).
    /// Caller has translated client `*` to SQL `%` via `globToLike`.
    actor_like: ?[]const u8 = null,
    /// Absolute epoch-millis lower bound on `created_at`. Mutually
    /// exclusive with `cursor` — the handler enforces.
    since_ms: ?i64 = null,
};

const ParsedCursor = struct {
    created_at: i64,
    event_id: []u8,
};

const CURSOR_EVENT_ID_MAX_LEN: usize = 128;

/// Build an opaque base64url cursor from the last (created_at, event_id)
/// of a page. Caller owns the returned slice.
pub fn makeCursor(alloc: std.mem.Allocator, created_at: i64, event_id: []const u8) ![]u8 {
    const plain = try std.fmt.allocPrint(alloc, "{d}:{s}", .{ created_at, event_id });
    defer alloc.free(plain);
    const encoded_len = base64.Encoder.calcSize(plain.len);
    const encoded = try alloc.alloc(u8, encoded_len);
    _ = base64.Encoder.encode(encoded, plain);
    return encoded;
}

/// Decode an opaque cursor. Caller owns `result.event_id`.
pub fn parseCursor(alloc: std.mem.Allocator, cursor: []const u8) !ParsedCursor {
    const decoded_len = base64.Decoder.calcSizeForSlice(cursor) catch return error.InvalidCursor;
    const plain = try alloc.alloc(u8, decoded_len);
    defer alloc.free(plain);
    base64.Decoder.decode(plain, cursor) catch return error.InvalidCursor;

    const sep = std.mem.indexOf(u8, plain, ":") orelse return error.InvalidCursor;
    const ts = std.fmt.parseInt(i64, plain[0..sep], 10) catch return error.InvalidCursor;
    const id_slice = plain[sep + 1 ..];
    if (id_slice.len == 0 or id_slice.len > CURSOR_EVENT_ID_MAX_LEN) return error.InvalidCursor;
    return .{ .created_at = ts, .event_id = try alloc.dupe(u8, id_slice) };
}

pub const SinceError = error{InvalidSince};

/// Parse a `since=` query value into an absolute epoch-millis lower
/// bound on `created_at`. Accepts:
///   - Go-style durations: `15s`, `30m`, `2h`, `7d` → now - duration
///   - RFC 3339 `YYYY-MM-DDTHH:MM:SSZ`             → literal epoch ms
///
/// `now_ms` is injected so tests stay deterministic.
pub fn parseSince(input: []const u8, now_ms: i64) SinceError!i64 {
    if (input.len == 0) return SinceError.InvalidSince;
    const last = input[input.len - 1];
    if (last == 's' or last == 'm' or last == 'h' or last == 'd') {
        const num = std.fmt.parseInt(i64, input[0 .. input.len - 1], 10) catch return SinceError.InvalidSince;
        if (num < 0) return SinceError.InvalidSince;
        const unit_ms: i64 = switch (last) {
            's' => 1_000,
            'm' => 60_000,
            'h' => 3_600_000,
            'd' => 86_400_000,
            else => unreachable,
        };
        return now_ms - num * unit_ms;
    }
    return parseRfc3339Z(input);
}

fn parseRfc3339Z(s: []const u8) SinceError!i64 {
    if (s.len != 20) return SinceError.InvalidSince;
    if (s[4] != '-' or s[7] != '-' or s[10] != 'T' or s[13] != ':' or s[16] != ':' or s[19] != 'Z') return SinceError.InvalidSince;
    const year = std.fmt.parseInt(i32, s[0..4], 10) catch return SinceError.InvalidSince;
    const month = std.fmt.parseInt(u8, s[5..7], 10) catch return SinceError.InvalidSince;
    const day = std.fmt.parseInt(u8, s[8..10], 10) catch return SinceError.InvalidSince;
    const hour = std.fmt.parseInt(u8, s[11..13], 10) catch return SinceError.InvalidSince;
    const minute = std.fmt.parseInt(u8, s[14..16], 10) catch return SinceError.InvalidSince;
    const second = std.fmt.parseInt(u8, s[17..19], 10) catch return SinceError.InvalidSince;
    if (month < 1 or month > 12) return SinceError.InvalidSince;
    if (day < 1 or day > 31) return SinceError.InvalidSince;
    if (hour > 23 or minute > 59 or second > 59) return SinceError.InvalidSince;

    const days = daysFromCivil(year, month, day);
    const seconds: i64 = @as(i64, days) * 86_400 + @as(i64, hour) * 3600 + @as(i64, minute) * 60 + @as(i64, second);
    return seconds * 1_000;
}

// Howard Hinnant's date algorithm: civil → days from 1970-01-01.
fn daysFromCivil(y_in: i32, m: u8, d: u8) i32 {
    const y_adj: i32 = if (m <= 2) y_in - 1 else y_in;
    const era: i32 = @divFloor(if (y_adj >= 0) y_adj else (y_adj - 399), 400);
    const yoe: u32 = @intCast(y_adj - era * 400);
    const m_i32: i32 = @intCast(m);
    const d_i32: i32 = @intCast(d);
    const m_offset: i32 = if (m_i32 > 2) -3 else 9;
    const doy: u32 = @intCast(@divFloor(153 * (m_i32 + m_offset) + 2, 5) + d_i32 - 1);
    const doe: u32 = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    return era * 146_097 + @as(i32, @intCast(doe)) - 719_468;
}

/// Translate a client glob (`steer:*`, `webhook:*`, `webhook:github`) to
/// a SQL LIKE pattern. Escapes `%` and `_` so they do not become
/// wildcards. `*` becomes `%`.
///
/// Caller owns the returned slice.
pub fn globToLike(alloc: std.mem.Allocator, glob: []const u8) ![]u8 {
    var out: std.ArrayList(u8) = .{};
    errdefer out.deinit(alloc);
    for (glob) |b| {
        switch (b) {
            '*' => try out.append(alloc, '%'),
            '%', '_' => {
                try out.append(alloc, '\\');
                try out.append(alloc, b);
            },
            else => try out.append(alloc, b),
        }
    }
    return out.toOwnedSlice(alloc);
}
