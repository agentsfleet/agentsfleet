// M25_001: Shared fixtures for Tier-1 zbench micro-benchmarks.
//
// All inputs live here so each bench_xxx fn in zbench_micro.zig stays under
// 30 lines and the fixtures themselves can be audited in one place.

const std = @import("std");
const webhook_verify = @import("zombie/webhook_verify.zig");
const HmacSha256 = std.crypto.auth.hmac.sha2.HmacSha256;

// ── activity_chunk_encode ─────────────────────────────────────────────────
// Representative chunk frame: a UUIDv7 event id, a 256-byte text payload
// (≈64 BPE tokens — typical LLM chunk batch), and the kind discriminator.
pub const CHUNK_ZOMBIE_ID = "019abcde-1234-7aaa-8bbb-abcdef012345";
pub const CHUNK_EVENT_ID = "019abcde-5678-7ccc-8ddd-abcdef012345";
pub const CHUNK_TEXT =
    "The quick brown fox jumps over the lazy dog. " ++
    "The quick brown fox jumps over the lazy dog. " ++
    "The quick brown fox jumps over the lazy dog. " ++
    "The quick brown fox jumps over the lazy dog. " ++
    "The quick brown fox jumps over the lazy dog. " ++
    "Pack my box with five dozen liquor jugs. The end.";

// ── progress_frame_decode ─────────────────────────────────────────────────
// Wire-shape JSON-RPC notification produced by the executor. Mirrors what
// `progress_callbacks.encodeProgress` emits for an agent_response_chunk;
// the bench feeds this verbatim into the transport read loop.
pub const PROGRESS_FRAME_BYTES =
    \\{"jsonrpc":"2.0","id":1,"method":"Progress","params":{"kind":"agent_response_chunk","text":"hello world streaming chunk payload of moderate size for the bench"}}
;

// ── 1.1 route_match ────────────────────────────────────────────────────────
// Representative paths covering every Route arm (one per group). Enough
// diversity that the worst-case fall-through path is exercised on each sweep.
pub const ROUTE_PATHS = [_][]const u8{
    "/healthz",
    "/readyz",
    "/metrics",
    "/v1/auth/sessions",
    "/v1/github/callback",
    "/v1/workspaces",
    "/v1/workspaces/019abcde-1234-7aaa-8bbb-abcdef012345/zombies",
    "/v1/workspaces/019abcde-1234-7aaa-8bbb-abcdef012345/zombies/019abcde-5678-7ccc-8ddd-abcdef012345/activity",
    "/v1/workspaces/019abcde-1234-7aaa-8bbb-abcdef012345/credentials",
    "/v1/webhooks/019abcde-1234-7aaa-8bbb-abcdef012345",
    "/v1/workspaces/019abcde-1234-7aaa-8bbb-abcdef012345/zombies/019abcde-5678-7ccc-8ddd-abcdef012345/memories",
    "/v1/execute",
};

// ── 1.2 error_registry_lookup ──────────────────────────────────────────────
// Mix of real registered codes + 2 unknowns so both the hit and UNKNOWN
// paths are exercised.
pub const ERROR_CODES = [_][]const u8{
    "UZ-REQ-001", "UZ-REQ-002", "UZ-AUTH-001", "UZ-AUTH-002", "UZ-AUTH-003",
    "UZ-AUTH-004", "UZ-AUTH-005", "UZ-AUTH-006", "UZ-INTERNAL-001", "UZ-INTERNAL-002",
    "UZ-INTERNAL-003", "UZ-UUIDV7-003", "UZ-UUIDV7-005", "UZ-UUIDV7-009", "UZ-UUIDV7-010",
    "UZ-UUIDV7-011", "UZ-UUIDV7-012",
    // Unknowns — exercise the StaticStringMap miss path.
    "UZ-DOES-NOT-EXIST-001", "UZ-ALSO-MISSING-002",
};

// ── 1.3 keyset_cursor_roundtrip ──────────────────────────────────────────
// 100 synthetic cursors. Variety in timestamp width and id length keeps the
// decimal parse from benchmarking a single register-fit case.
fn buildCursors(comptime n: usize) [n][]const u8 {
    @setEvalBranchQuota(1_000_000);
    var out: [n][]const u8 = undefined;
    for (0..n) |i| {
        const ts: u64 = 1_700_000_000_000 + @as(u64, i) * 37;
        // Hand-rolled hex id so the slice is comptime-known.
        out[i] = std.fmt.comptimePrint("{d}:019abcde-{x:0>4}-7aaa-8bbb-abcdef012345", .{ ts, i });
    }
    return out;
}
pub const CURSORS = buildCursors(100);

// ── 1.4 json_encode_response ───────────────────────────────────────────────
pub const ZombieRow = struct {
    id: []const u8,
    workspace_id: []const u8,
    name: []const u8,
    status: []const u8,
    created_at: i64,
    updated_at: i64,
};

pub const ZOMBIE_PAGE = [_]ZombieRow{
    .{ .id = "019abcde-0001-7aaa-8bbb-abcdef012345", .workspace_id = "019wsps-7aaa-8bbb-abcdef012345", .name = "zombie-alpha",   .status = "running",  .created_at = 1_700_000_000_000, .updated_at = 1_700_000_030_000 },
    .{ .id = "019abcde-0002-7aaa-8bbb-abcdef012345", .workspace_id = "019wsps-7aaa-8bbb-abcdef012345", .name = "zombie-beta",    .status = "running",  .created_at = 1_700_000_001_000, .updated_at = 1_700_000_031_000 },
    .{ .id = "019abcde-0003-7aaa-8bbb-abcdef012345", .workspace_id = "019wsps-7aaa-8bbb-abcdef012345", .name = "zombie-gamma",   .status = "paused",   .created_at = 1_700_000_002_000, .updated_at = 1_700_000_032_000 },
    .{ .id = "019abcde-0004-7aaa-8bbb-abcdef012345", .workspace_id = "019wsps-7aaa-8bbb-abcdef012345", .name = "zombie-delta",   .status = "running",  .created_at = 1_700_000_003_000, .updated_at = 1_700_000_033_000 },
    .{ .id = "019abcde-0005-7aaa-8bbb-abcdef012345", .workspace_id = "019wsps-7aaa-8bbb-abcdef012345", .name = "zombie-epsilon", .status = "stopped",  .created_at = 1_700_000_004_000, .updated_at = 1_700_000_034_000 },
    .{ .id = "019abcde-0006-7aaa-8bbb-abcdef012345", .workspace_id = "019wsps-7aaa-8bbb-abcdef012345", .name = "zombie-zeta",    .status = "running",  .created_at = 1_700_000_005_000, .updated_at = 1_700_000_035_000 },
    .{ .id = "019abcde-0007-7aaa-8bbb-abcdef012345", .workspace_id = "019wsps-7aaa-8bbb-abcdef012345", .name = "zombie-eta",     .status = "running",  .created_at = 1_700_000_006_000, .updated_at = 1_700_000_036_000 },
    .{ .id = "019abcde-0008-7aaa-8bbb-abcdef012345", .workspace_id = "019wsps-7aaa-8bbb-abcdef012345", .name = "zombie-theta",   .status = "paused",   .created_at = 1_700_000_007_000, .updated_at = 1_700_000_037_000 },
    .{ .id = "019abcde-0009-7aaa-8bbb-abcdef012345", .workspace_id = "019wsps-7aaa-8bbb-abcdef012345", .name = "zombie-iota",    .status = "running",  .created_at = 1_700_000_008_000, .updated_at = 1_700_000_038_000 },
    .{ .id = "019abcde-000a-7aaa-8bbb-abcdef012345", .workspace_id = "019wsps-7aaa-8bbb-abcdef012345", .name = "zombie-kappa",   .status = "running",  .created_at = 1_700_000_009_000, .updated_at = 1_700_000_039_000 },
};

// ── 1.7 webhook_signature_verify ───────────────────────────────────────────
// A ~1 KB random-ish payload — real webhooks range from a few hundred bytes
// to ~64 KiB; 1 KB is representative of Slack/GitHub event bodies.
pub const WEBHOOK_SECRET = "test_signing_secret_not_real_for_bench_0000";
pub const WEBHOOK_BODY = mkBody(1024);

fn mkBody(comptime n: usize) [n]u8 {
    @setEvalBranchQuota(n * 10);
    var buf: [n]u8 = undefined;
    for (0..n) |i| buf[i] = @intCast((i * 31 + 7) & 0xff);
    return buf;
}

// Precomputed signature over WEBHOOK_BODY — format matches
// webhook_verify.GITHUB (prefix "sha256=" + 64 hex chars).
pub const WEBHOOK_SIGNATURE = blk: {
    @setEvalBranchQuota(100_000);
    var mac: [HmacSha256.mac_length]u8 = undefined;
    var hmac = HmacSha256.init(WEBHOOK_SECRET);
    hmac.update(&WEBHOOK_BODY);
    hmac.final(&mac);
    const hex = std.fmt.bytesToHex(mac, .lower);
    break :blk "sha256=" ++ hex;
};

comptime {
    // Catch silent drift: a format change in GITHUB would invalidate the
    // precomputed signature, producing a benchmark that always hits the
    // reject path and doesn't measure the work we think it does.
    std.debug.assert(webhook_verify.GITHUB.prefix.len == "sha256=".len);
    std.debug.assert(WEBHOOK_SIGNATURE.len == "sha256=".len + 64);
}
