// Tests extracted from id_format.zig to keep that file under 350 lines.
// Pattern: approval_gate_test.zig.

const std = @import("std");
const id = @import("id_format.zig");

test "generate ids support both formats" {
    const alloc = std.testing.allocator;
    const tenant_id = try id.generateTenantId(alloc);
    defer alloc.free(tenant_id);
    try std.testing.expect(id.isSupportedTenantId(tenant_id));

    const workspace_id = try id.generateWorkspaceId(alloc);
    defer alloc.free(workspace_id);
    try std.testing.expect(id.isSupportedWorkspaceId(workspace_id));

    const spec_id = try id.generateSpecId(alloc);
    defer alloc.free(spec_id);
    try std.testing.expect(id.isSupportedSpecId(spec_id));

    const run_id = try id.generateRunId(alloc);
    defer alloc.free(run_id);
    try std.testing.expect(id.isSupportedRunId(run_id));

    const profile_id = try id.generateAgentId(alloc);
    defer alloc.free(profile_id);
    try std.testing.expect(id.isSupportedProfileId(profile_id));

    const pver_id = try id.generateProfileVersionId(alloc);
    defer alloc.free(pver_id);
    try std.testing.expect(id.isSupportedProfileVersionId(pver_id));

    const cjob_id = try id.generateCompileJobId(alloc);
    defer alloc.free(cjob_id);
    try std.testing.expect(id.isSupportedCompileJobId(cjob_id));

    const artifact_id = try id.generateProfileLinkageArtifactId(alloc);
    defer alloc.free(artifact_id);
    try std.testing.expect(id.isSupportedProfileLinkageArtifactId(artifact_id));

    const snapshot_id = try id.generateEntitlementSnapshotId(alloc);
    defer alloc.free(snapshot_id);
    try std.testing.expect(id.isSupportedEntitlementSnapshotId(snapshot_id));
}

test "uuidv7 validator accepts canonical v7 variant 10xx" {
    try std.testing.expect(id.isUuidV7("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99"));
    try std.testing.expect(!id.isUuidV7("0195b4ba-8d3a-6f13-8abc-2b3e1e0a6f99"));
}

test "validators reject legacy prefixed ids after uuid cutover" {
    try std.testing.expect(!id.isSupportedRunId("not-a-uuid"));
    try std.testing.expect(!id.isSupportedProfileVersionId("missing-uuid-shape"));
    try std.testing.expect(!id.isSupportedCompileJobId("0195b4ba8d3a7f138abc2b3e1e0a6f99"));
}

test "all new id generators produce valid uuidv7" {
    const alloc = std.testing.allocator;
    inline for (.{
        id.generateTransitionId,      id.generateArtifactId,             id.generateUsageLedgerId,
        id.generateWorkspaceMemoryId, id.generatePolicyEventId,          id.generateSideEffectId,
        id.generateOutboxId,          id.generateBillingDeliveryId,      id.generateVaultSecretId,
        id.generatePromptLifecycleEventId, id.generatePlatformLlmKeyId,
    }) |gen| {
        const idd = try gen(alloc);
        defer alloc.free(idd);
        try std.testing.expect(id.isUuidV7(idd));
    }
}

test "generated ids are unique across calls" {
    const alloc = std.testing.allocator;
    const id1 = try id.generateTransitionId(alloc);
    defer alloc.free(id1);
    const id2 = try id.generateTransitionId(alloc);
    defer alloc.free(id2);
    try std.testing.expect(!std.mem.eql(u8, id1, id2));
}

test "all generated ids are 36 bytes" {
    const alloc = std.testing.allocator;
    const idd = try id.generateTransitionId(alloc);
    defer alloc.free(idd);
    try std.testing.expectEqual(@as(usize, 36), idd.len);
}

test "T2: version nibble and variant bits are correctly set across all generators" {
    const alloc = std.testing.allocator;
    inline for (.{
        id.generateTransitionId,      id.generateArtifactId,             id.generateUsageLedgerId,
        id.generateWorkspaceMemoryId, id.generatePolicyEventId,          id.generateSideEffectId,
        id.generateOutboxId,          id.generateBillingDeliveryId,      id.generateVaultSecretId,
        id.generatePromptLifecycleEventId, id.generatePlatformLlmKeyId,
    }) |gen| {
        const idd = try gen(alloc);
        defer alloc.free(idd);
        try std.testing.expectEqual(@as(u8, '7'), idd[14]);
        try std.testing.expect(idd[19] == '8' or idd[19] == '9' or idd[19] == 'a' or idd[19] == 'b');
        try std.testing.expectEqual(@as(u8, '-'), idd[8]);
        try std.testing.expectEqual(@as(u8, '-'), idd[13]);
        try std.testing.expectEqual(@as(u8, '-'), idd[18]);
        try std.testing.expectEqual(@as(u8, '-'), idd[23]);
    }
}

test "T2: ids from different generators are distinct" {
    const alloc = std.testing.allocator;
    const a = try id.generateTransitionId(alloc);
    defer alloc.free(a);
    const b = try id.generateArtifactId(alloc);
    defer alloc.free(b);
    const c = try id.generateUsageLedgerId(alloc);
    defer alloc.free(c);
    const d = try id.generateSideEffectId(alloc);
    defer alloc.free(d);
    try std.testing.expect(!std.mem.eql(u8, a, b));
    try std.testing.expect(!std.mem.eql(u8, b, c));
    try std.testing.expect(!std.mem.eql(u8, c, d));
}

test "T2: all hex chars are lowercase" {
    const alloc = std.testing.allocator;
    const idd = try id.generateTransitionId(alloc);
    defer alloc.free(idd);
    for (idd) |c| {
        if (c == '-') continue;
        try std.testing.expect((c >= '0' and c <= '9') or (c >= 'a' and c <= 'f'));
    }
}

test "T3: generator returns OutOfMemory when allocator fails" {
    var fa = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = 0 });
    const result = id.generateTransitionId(fa.allocator());
    try std.testing.expectError(error.OutOfMemory, result);
}

test "T3: isUuidV7 rejects wrong length strings" {
    try std.testing.expect(!id.isUuidV7(""));
    try std.testing.expect(!id.isUuidV7("short"));
    try std.testing.expect(!id.isUuidV7("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f9")); // 35 chars
    try std.testing.expect(!id.isUuidV7("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f999")); // 37 chars
}

test "T3: isUuidV7 rejects non-hex characters" {
    try std.testing.expect(!id.isUuidV7("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6gzz"));
    try std.testing.expect(!id.isUuidV7("zzzzzzzz-zzzz-7zzz-8zzz-zzzzzzzzzzzz"));
}

test "T3: isUuidV7 rejects wrong variant nibble" {
    try std.testing.expect(!id.isUuidV7("0195b4ba-8d3a-7f13-0abc-2b3e1e0a6f99"));
    try std.testing.expect(!id.isUuidV7("0195b4ba-8d3a-7f13-cabc-2b3e1e0a6f99"));
}

test "T5: concurrent generation produces no duplicates" {
    const num_threads = 8;
    const ids_per_thread = 64;
    const total = num_threads * ids_per_thread;

    const Context = struct {
        ids: [total][]const u8 = undefined,

        fn worker(self: *@This(), base: usize) void {
            const alloc = std.testing.allocator;
            for (0..ids_per_thread) |i| {
                self.ids[base + i] = id.generateTransitionId(alloc) catch "FAILED";
            }
        }
    };
    var ctx: Context = .{};

    var threads: [num_threads]std.Thread = undefined;
    for (0..num_threads) |t| {
        threads[t] = try std.Thread.spawn(.{}, Context.worker, .{ &ctx, t * ids_per_thread });
    }
    for (&threads) |t| t.join();

    defer for (&ctx.ids) |idd| {
        if (!std.mem.eql(u8, idd, "FAILED")) std.testing.allocator.free(idd);
    };

    for (&ctx.ids) |idd| {
        try std.testing.expect(!std.mem.eql(u8, idd, "FAILED"));
        try std.testing.expect(id.isUuidV7(idd));
    }
    for (0..total) |i| {
        for (i + 1..total) |j| {
            try std.testing.expect(!std.mem.eql(u8, ctx.ids[i], ctx.ids[j]));
        }
    }
}
