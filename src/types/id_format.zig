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

pub fn generateProfileId(alloc: std.mem.Allocator) ![]const u8 {
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

fn allocUuidV7(alloc: std.mem.Allocator) ![]const u8 {
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

test "generate ids support both formats" {
    const alloc = std.testing.allocator;
    const tenant_id = try generateTenantId(alloc);
    defer alloc.free(tenant_id);
    try std.testing.expect(isSupportedTenantId(tenant_id));

    const workspace_id = try generateWorkspaceId(alloc);
    defer alloc.free(workspace_id);
    try std.testing.expect(isSupportedWorkspaceId(workspace_id));

    const spec_id = try generateSpecId(alloc);
    defer alloc.free(spec_id);
    try std.testing.expect(isSupportedSpecId(spec_id));

    const run_id = try generateRunId(alloc);
    defer alloc.free(run_id);
    try std.testing.expect(isSupportedRunId(run_id));

    const profile_id = try generateProfileId(alloc);
    defer alloc.free(profile_id);
    try std.testing.expect(isSupportedProfileId(profile_id));

    const pver_id = try generateProfileVersionId(alloc);
    defer alloc.free(pver_id);
    try std.testing.expect(isSupportedProfileVersionId(pver_id));

    const cjob_id = try generateCompileJobId(alloc);
    defer alloc.free(cjob_id);
    try std.testing.expect(isSupportedCompileJobId(cjob_id));

    const artifact_id = try generateProfileLinkageArtifactId(alloc);
    defer alloc.free(artifact_id);
    try std.testing.expect(isSupportedProfileLinkageArtifactId(artifact_id));

    const snapshot_id = try generateEntitlementSnapshotId(alloc);
    defer alloc.free(snapshot_id);
    try std.testing.expect(isSupportedEntitlementSnapshotId(snapshot_id));
}

test "uuidv7 validator accepts canonical v7 variant 10xx" {
    try std.testing.expect(isUuidV7("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99"));
    try std.testing.expect(!isUuidV7("0195b4ba-8d3a-6f13-8abc-2b3e1e0a6f99"));
}

test "validators reject legacy prefixed ids after uuid cutover" {
    try std.testing.expect(!isSupportedRunId("not-a-uuid"));
    try std.testing.expect(!isSupportedProfileVersionId("missing-uuid-shape"));
    try std.testing.expect(!isSupportedCompileJobId("0195b4ba8d3a7f138abc2b3e1e0a6f99"));
}
