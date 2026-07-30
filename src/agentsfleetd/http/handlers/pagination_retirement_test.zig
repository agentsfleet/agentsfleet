// Page-number pagination is retired from the daemon: no handler parses a
// `page`/`page_size` pair through a shared helper, and the helper itself is
// gone. Embedding the former callers pins the property at compile+test time —
// re-introducing the helper or its call shape fails here before review.

const std = @import("std");

test "test_page_param_helper_is_gone" {
    const former_callers = [_][]const u8{
        @embedFile("api_keys/list.zig"),
        @embedFile("fleet/runners_list.zig"),
        @embedFile("fleet/runner_events.zig"),
    };
    for (former_callers) |src| {
        try std.testing.expect(std.mem.indexOf(u8, src, "parsePageParams") == null);
    }
}
