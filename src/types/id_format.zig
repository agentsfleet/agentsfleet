const std = @import("std");

pub fn generateRunId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateTenantId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateWorkspaceId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateSpecId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateAgentId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateProfileVersionId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateCompileJobId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateProfileLinkageArtifactId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateEntitlementSnapshotId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateTransitionId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateArtifactId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateUsageLedgerId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateWorkspaceMemoryId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generatePolicyEventId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateSideEffectId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateOutboxId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateBillingDeliveryId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateVaultSecretId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateSkillSecretId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generatePromptLifecycleEventId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateScoreId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateGateResultId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generatePlatformLlmKeyId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateZombieId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateZombieSessionId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn generateActivityEventId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

// M8_001: workspace-level provider integration record ID
pub fn generateIntegrationId(alloc: std.mem.Allocator) ![]const u8 {
    return allocUuidV7(alloc);
}

pub fn isSupportedAgentId(id: []const u8) bool {
    return isUuidV7(id);
}

pub fn isSupportedRunId(id: []const u8) bool {
    return isUuidV7(id);
}

pub fn isSupportedTenantId(id: []const u8) bool {
    return isUuidV7(id);
}

pub fn isSupportedWorkspaceId(id: []const u8) bool {
    return isUuidV7(id);
}

pub fn isSupportedSpecId(id: []const u8) bool {
    return isUuidV7(id);
}

pub fn isSupportedProfileId(id: []const u8) bool {
    return isUuidV7(id);
}

pub fn isSupportedProfileVersionId(id: []const u8) bool {
    return isUuidV7(id);
}

pub fn isSupportedCompileJobId(id: []const u8) bool {
    return isUuidV7(id);
}

pub fn isSupportedProfileLinkageArtifactId(id: []const u8) bool {
    return isUuidV7(id);
}

pub fn isSupportedEntitlementSnapshotId(id: []const u8) bool {
    return isUuidV7(id);
}

pub fn isUuidV7(id: []const u8) bool {
    if (!isCanonicalUuid(id)) return false;
    if (id[14] != '7') return false;
    return switch (id[19]) {
        '8', '9', 'a', 'b', 'A', 'B' => true,
        else => false,
    };
}

pub fn allocUuidV7(alloc: std.mem.Allocator) ![]const u8 {
    var raw: [16]u8 = undefined;
    std.crypto.random.bytes(&raw);

    const ts_ms: u64 = @intCast(std.time.milliTimestamp());
    raw[0] = @intCast((ts_ms >> 40) & 0xff);
    raw[1] = @intCast((ts_ms >> 32) & 0xff);
    raw[2] = @intCast((ts_ms >> 24) & 0xff);
    raw[3] = @intCast((ts_ms >> 16) & 0xff);
    raw[4] = @intCast((ts_ms >> 8) & 0xff);
    raw[5] = @intCast(ts_ms & 0xff);

    // Version 7 in upper nibble.
    raw[6] = (raw[6] & 0x0f) | 0x70;
    // RFC 4122 variant in top bits.
    raw[8] = (raw[8] & 0x3f) | 0x80;

    const hex = std.fmt.bytesToHex(raw, .lower);
    return std.fmt.allocPrint(alloc, "{s}-{s}-{s}-{s}-{s}", .{
        hex[0..8],
        hex[8..12],
        hex[12..16],
        hex[16..20],
        hex[20..32],
    });
}

fn isCanonicalUuid(id: []const u8) bool {
    if (id.len != 36) return false;
    if (id[8] != '-' or id[13] != '-' or id[18] != '-' or id[23] != '-') return false;
    for (id, 0..) |c, idx| {
        if (idx == 8 or idx == 13 or idx == 18 or idx == 23) continue;
        if (!isHex(c)) return false;
    }
    return true;
}

fn isHex(c: u8) bool {
    return (c >= '0' and c <= '9') or
        (c >= 'a' and c <= 'f') or
        (c >= 'A' and c <= 'F');
}

// Tests in id_format_test.zig (RULE FLL: keep file ≤ 350 lines).
test {
    _ = @import("id_format_test.zig");
}
