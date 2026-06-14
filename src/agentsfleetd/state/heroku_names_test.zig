const std = @import("std");
const heroku_names = @import("heroku_names.zig");

test "heroku_names: shape matches {adj}-{noun}-{3digit}" {
    const alloc = std.testing.allocator;
    const name = try heroku_names.generate(alloc);
    defer alloc.free(name);

    var it = std.mem.splitScalar(u8, name, '-');
    const adj = it.next() orelse return error.TestUnexpectedResult;
    const noun = it.next() orelse return error.TestUnexpectedResult;
    const suffix = it.next() orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(@as(?[]const u8, null), it.next());

    try std.testing.expect(inList(&heroku_names.ADJECTIVES, adj));
    try std.testing.expect(inList(&heroku_names.NOUNS, noun));
    try std.testing.expectEqual(@as(usize, 3), suffix.len);
    const n = try std.fmt.parseInt(u32, suffix, 10);
    try std.testing.expect(n < heroku_names.SUFFIX_MAX);
    for (suffix) |c| try std.testing.expect(c >= '0' and c <= '9');
}

test "heroku_names: 100 iterations leak-free" {
    const alloc = std.testing.allocator;
    var i: usize = 0;
    while (i < 100) : (i += 1) {
        const name = try heroku_names.generate(alloc);
        alloc.free(name);
    }
}

test "heroku_names: generates varied names" {
    const alloc = std.testing.allocator;
    var seen = std.StringHashMap(void).init(alloc);
    defer {
        var it = seen.iterator();
        while (it.next()) |entry| alloc.free(entry.key_ptr.*);
        seen.deinit();
    }
    var i: usize = 0;
    while (i < 32) : (i += 1) {
        const name = try heroku_names.generate(alloc);
        errdefer alloc.free(name);
        try seen.put(name, {});
    }
    try std.testing.expect(seen.count() > 1);
}

test "heroku_names: every char lowercase ascii or digit or hyphen" {
    const alloc = std.testing.allocator;
    var i: usize = 0;
    while (i < 64) : (i += 1) {
        const name = try heroku_names.generate(alloc);
        defer alloc.free(name);
        for (name) |c| {
            const ok = (c >= 'a' and c <= 'z') or (c >= '0' and c <= '9') or c == '-';
            try std.testing.expect(ok);
        }
    }
}

fn inList(list: []const []const u8, needle: []const u8) bool {
    for (list) |entry| {
        if (std.mem.eql(u8, entry, needle)) return true;
    }
    return false;
}
