const std = @import("std");
const secure_memory = @import("secure_memory.zig");

test "secure memory free hands zeroed bytes to the child allocator" {
    var backing: [64]u8 = undefined;
    var fixed = std.heap.FixedBufferAllocator.init(&backing);
    const alloc = fixed.allocator();
    const secret = try alloc.alloc(u8, 32);
    @memset(secret, 0x7B);
    const offset = @intFromPtr(secret.ptr) - @intFromPtr(backing[0..].ptr);

    secure_memory.freeBytes(alloc, secret);

    try std.testing.expectEqualSlices(u8, &([_]u8{0} ** 32), backing[offset..][0..32]);
}

test "secure memory free accepts an empty buffer" {
    secure_memory.freeBytes(std.testing.allocator, &.{});
}

test "vault and secret write choke points use secure memory release" {
    const vault_source = @embedFile("../state/vault.zig");
    const secret_handler_source = @embedFile("../http/handlers/fleets/secrets.zig");
    const release_plaintext = "defer secure_memory.freeBytes(alloc, plaintext);";

    try std.testing.expect(std.mem.indexOf(u8, vault_source, release_plaintext) != null);
    try std.testing.expectEqual(
        @as(usize, 2),
        std.mem.count(u8, secret_handler_source, release_plaintext),
    );
}
