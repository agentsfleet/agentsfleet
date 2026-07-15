const std = @import("std");
const model = @import("model.zig");

test "model: named schedule states round-trip and reject unknown values" {
    inline for (std.meta.tags(model.DesiredStatus)) |tag| {
        try std.testing.expectEqual(tag, model.DesiredStatus.fromSlice(tag.toSlice()).?);
    }
    inline for (std.meta.tags(model.SyncStatus)) |tag| {
        try std.testing.expectEqual(tag, model.SyncStatus.fromSlice(tag.toSlice()).?);
    }
    inline for (std.meta.tags(model.Source)) |tag| {
        try std.testing.expectEqual(tag, model.Source.fromSlice(tag.toSlice()).?);
    }
    try std.testing.expectEqual(@as(?model.Source, null), model.Source.fromSlice("local"));
    try std.testing.expectEqual(@as(?model.DesiredStatus, null), model.DesiredStatus.fromSlice("missing"));
    try std.testing.expectEqual(@as(?model.SyncStatus, null), model.SyncStatus.fromSlice("pending"));
}

test "model: fleet schedule cap and timezone defaults are stable" {
    try std.testing.expectEqual(@as(usize, 32), model.MAX_SCHEDULES_PER_FLEET);
    try std.testing.expectEqualStrings("UTC", model.DEFAULT_TIMEZONE);
}
