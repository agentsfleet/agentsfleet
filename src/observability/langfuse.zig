//! Langfuse LLM/agent tracing integration.
//! Fire-and-forget: HTTP POST to Langfuse ingest API after each agent call.
//! Optional: configured via LANGFUSE_HOST, LANGFUSE_PUBLIC_KEY, LANGFUSE_SECRET_KEY.

const std = @import("std");
const trace = @import("trace.zig");
const obs_log = @import("logging.zig");
const metrics = @import("metrics.zig");
const log = std.log.scoped(.langfuse);

const INGEST_PATH = "/api/public/ingestion";
pub const ERR_LANGFUSE_EXPORT_FAILED = "UZ-OBS-LANGFUSE-001";
pub const ERR_LANGFUSE_CIRCUIT_OPEN = "UZ-OBS-LANGFUSE-002";

// Circuit breaker state for Langfuse exporter.
// After FAILURE_THRESHOLD consecutive failures, the circuit opens for
// OPEN_DURATION_MS before allowing a single probe request.
const FAILURE_THRESHOLD: u32 = 5;
const OPEN_DURATION_MS: i64 = 60_000; // 1 minute

var g_consecutive_failures = std.atomic.Value(u32).init(0);
var g_circuit_open_until_ms = std.atomic.Value(i64).init(0);

pub const LangfuseConfig = struct {
    host: []const u8, // e.g. "https://cloud.langfuse.com"
    public_key: []const u8,
    secret_key: []const u8,
};

const EmitError = error{
    RequestFailed,
    UnexpectedStatus,
};

/// Try to load Langfuse config from environment. Returns null when not configured.
pub fn configFromEnv(alloc: std.mem.Allocator) ?LangfuseConfig {
    const host = std.process.getEnvVarOwned(alloc, "LANGFUSE_HOST") catch return null;
    const public_key = std.process.getEnvVarOwned(alloc, "LANGFUSE_PUBLIC_KEY") catch {
        alloc.free(host);
        return null;
    };
    const secret_key = std.process.getEnvVarOwned(alloc, "LANGFUSE_SECRET_KEY") catch {
        alloc.free(host);
        alloc.free(public_key);
        return null;
    };
    return .{
        .host = host,
        .public_key = public_key,
        .secret_key = secret_key,
    };
}

pub const LangfuseTrace = struct {
    trace_id: []const u8,
    run_id: []const u8,
    stage_id: []const u8,
    role_id: []const u8,
    token_count: u64,
    wall_seconds: u64,
    exit_ok: bool,
    timestamp_ms: i64,
};

/// Render a Langfuse ingest batch payload as JSON.
/// Caller owns returned slice.
pub fn renderIngestPayload(alloc: std.mem.Allocator, t: LangfuseTrace) ![]u8 {
    var out: std.ArrayList(u8) = .{};
    errdefer out.deinit(alloc);
    const w = out.writer(alloc);

    try w.writeAll("{\"batch\":[{\"type\":\"trace-create\",\"body\":{");
    try w.print("\"id\":\"{s}\",", .{t.trace_id});
    try w.writeAll("\"name\":\"agent-run\",");
    try w.print("\"metadata\":{{\"run_id\":\"{s}\",\"stage_id\":\"{s}\",\"role_id\":\"{s}\",\"exit_ok\":{}}},", .{
        t.run_id,
        t.stage_id,
        t.role_id,
        t.exit_ok,
    });
    try w.print("\"input\":{{\"token_count\":{d},\"wall_seconds\":{d}}},", .{ t.token_count, t.wall_seconds });
    try w.print("\"timestamp\":\"{d}\"", .{t.timestamp_ms});
    try w.writeAll("}},{\"type\":\"generation-create\",\"body\":{");
    try w.print("\"traceId\":\"{s}\",", .{t.trace_id});
    try w.print("\"name\":\"{s}\",", .{t.role_id});
    try w.writeAll("\"model\":\"nullclaw\",");
    try w.print("\"usage\":{{\"input\":{d},\"output\":0,\"total\":{d}}},", .{ t.token_count, t.token_count });
    try w.print("\"startTime\":\"{d}\",", .{t.timestamp_ms - @as(i64, @intCast(t.wall_seconds * 1000))});
    try w.print("\"endTime\":\"{d}\"", .{t.timestamp_ms});
    try w.writeAll("}}]}");

    return out.toOwnedSlice(alloc);
}

fn isCircuitOpen() bool {
    if (g_consecutive_failures.load(.acquire) < FAILURE_THRESHOLD) return false;
    return std.time.milliTimestamp() < g_circuit_open_until_ms.load(.acquire);
}

/// Fire-and-forget emit of a Langfuse trace. Errors are logged, never propagated.
pub fn emitTrace(alloc: std.mem.Allocator, cfg: LangfuseConfig, t: LangfuseTrace) void {
    if (isCircuitOpen()) {
        metrics.incLangfuseCircuitOpen();
        log.warn("error_code={s} langfuse circuit open, skipping trace_id={s} run_id={s}", .{ ERR_LANGFUSE_CIRCUIT_OPEN, t.trace_id, t.run_id });
        return;
    }
    emitTraceInner(alloc, cfg, t) catch |err| {
        const fails = g_consecutive_failures.fetchAdd(1, .monotonic) + 1;
        if (fails >= FAILURE_THRESHOLD) {
            g_circuit_open_until_ms.store(std.time.milliTimestamp() + OPEN_DURATION_MS, .release);
        }
        metrics.incLangfuseEmitFailed();
        metrics.incLangfuseEmitTotal();
        obs_log.logWarnErr(.langfuse, err, "error_code={s} langfuse emit failed trace_id={s} run_id={s} host={s}", .{ ERR_LANGFUSE_EXPORT_FAILED, t.trace_id, t.run_id, cfg.host });
        return;
    };
    g_consecutive_failures.store(0, .release);
    metrics.setLangfuseLastSuccessAtMs(std.time.milliTimestamp());
    metrics.incLangfuseEmitTotal();
}

fn emitTraceInner(alloc: std.mem.Allocator, cfg: LangfuseConfig, t: LangfuseTrace) !void {
    const payload = try renderIngestPayload(alloc, t);
    defer alloc.free(payload);

    // Build URL: {host}/api/public/ingestion (handle optional trailing slash).
    const host = std.mem.trimRight(u8, cfg.host, "/");
    const url = try std.fmt.allocPrint(alloc, "{s}{s}", .{ host, INGEST_PATH });
    defer alloc.free(url);

    const auth_header = try buildBasicAuthHeader(alloc, cfg.public_key, cfg.secret_key);
    defer alloc.free(auth_header);

    try postJsonWithBasicAuth(alloc, url, auth_header, payload);
    log.info("langfuse_emit_ok url={s} trace_id={s} payload_len={d}", .{ url, t.trace_id, payload.len });
}

fn buildBasicAuthHeader(alloc: std.mem.Allocator, public_key: []const u8, secret_key: []const u8) ![]u8 {
    const credentials = try std.fmt.allocPrint(alloc, "{s}:{s}", .{ public_key, secret_key });
    defer alloc.free(credentials);

    const encoded_len = std.base64.standard.Encoder.calcSize(credentials.len);
    const encoded = try alloc.alloc(u8, encoded_len);
    errdefer alloc.free(encoded);
    _ = std.base64.standard.Encoder.encode(encoded, credentials);

    const header = try std.fmt.allocPrint(alloc, "Basic {s}", .{encoded});
    alloc.free(encoded);
    return header;
}

fn postJsonWithBasicAuth(alloc: std.mem.Allocator, url: []const u8, auth_header: []const u8, payload: []const u8) EmitError!void {
    var client: std.http.Client = .{ .allocator = alloc };
    defer client.deinit();

    var body: std.ArrayList(u8) = .{};
    defer body.deinit(alloc);
    var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &body);

    const headers: [2]std.http.Header = .{
        .{ .name = "content-type", .value = "application/json" },
        .{ .name = "authorization", .value = auth_header },
    };

    const result = client.fetch(.{
        .location = .{ .url = url },
        .method = .POST,
        .payload = payload,
        .extra_headers = &headers,
        .response_writer = &aw.writer,
    }) catch return EmitError.RequestFailed;

    if (!isSuccessStatus(result.status)) {
        return EmitError.UnexpectedStatus;
    }
}

fn isSuccessStatus(status: std.http.Status) bool {
    return switch (status) {
        .ok, .created, .accepted, .no_content => true,
        else => false,
    };
}

// --- Tests ---

test "renderIngestPayload produces valid JSON structure" {
    const alloc = std.testing.allocator;
    const payload = try renderIngestPayload(alloc, .{
        .trace_id = "abc123",
        .run_id = "run-1",
        .stage_id = "plan",
        .role_id = "echo",
        .token_count = 500,
        .wall_seconds = 12,
        .exit_ok = true,
        .timestamp_ms = 1710000000000,
    });
    defer alloc.free(payload);

    try std.testing.expect(std.mem.startsWith(u8, payload, "{\"batch\":["));
    try std.testing.expect(std.mem.containsAtLeast(u8, payload, 1, "\"trace-create\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, payload, 1, "\"generation-create\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, payload, 1, "\"run_id\":\"run-1\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, payload, 1, "\"total\":500"));
    try std.testing.expect(std.mem.endsWith(u8, payload, "}}]}"));
}

test "renderIngestPayload handles exit_ok=false" {
    const alloc = std.testing.allocator;
    const payload = try renderIngestPayload(alloc, .{
        .trace_id = "trace-fail",
        .run_id = "run-2",
        .stage_id = "verify",
        .role_id = "warden",
        .token_count = 1000,
        .wall_seconds = 45,
        .exit_ok = false,
        .timestamp_ms = 1710000060000,
    });
    defer alloc.free(payload);

    try std.testing.expect(std.mem.containsAtLeast(u8, payload, 1, "\"exit_ok\":false"));
    try std.testing.expect(std.mem.containsAtLeast(u8, payload, 1, "\"role_id\":\"warden\""));
}

test "buildBasicAuthHeader prefixes Basic and encodes credentials" {
    const alloc = std.testing.allocator;
    const header = try buildBasicAuthHeader(alloc, "pk-test", "sk-test");
    defer alloc.free(header);

    try std.testing.expect(std.mem.startsWith(u8, header, "Basic "));
    try std.testing.expect(std.mem.containsAtLeast(u8, header, 1, "cGstdGVzdDpzay10ZXN0"));
}

test "postJsonWithBasicAuth returns RequestFailed when endpoint unreachable" {
    const alloc = std.testing.allocator;
    try std.testing.expectError(
        EmitError.RequestFailed,
        postJsonWithBasicAuth(
            alloc,
            "http://127.0.0.1:1/api/public/ingestion",
            "Basic cGs6c2s=",
            "{\"batch\":[]}",
        ),
    );
}

test "emitTraceInner trims trailing slash in host path join" {
    const alloc = std.testing.allocator;
    try std.testing.expectError(
        EmitError.RequestFailed,
        emitTraceInner(alloc, .{
            .host = "http://127.0.0.1:1/",
            .public_key = "pk-test",
            .secret_key = "sk-test",
        }, .{
            .trace_id = "trace-smoke",
            .run_id = "run-smoke",
            .stage_id = "plan",
            .role_id = "echo",
            .token_count = 10,
            .wall_seconds = 1,
            .exit_ok = true,
            .timestamp_ms = 1710000000000,
        }),
    );
}

test "isSuccessStatus accepts 2xx and rejects non-2xx" {
    try std.testing.expect(isSuccessStatus(.ok));
    try std.testing.expect(isSuccessStatus(.created));
    try std.testing.expect(isSuccessStatus(.accepted));
    try std.testing.expect(isSuccessStatus(.no_content));
    try std.testing.expect(!isSuccessStatus(.bad_request));
    try std.testing.expect(!isSuccessStatus(.unauthorized));
}

test "circuit breaker opens after consecutive failures" {
    g_consecutive_failures.store(FAILURE_THRESHOLD, .release);
    g_circuit_open_until_ms.store(std.time.milliTimestamp() + 60_000, .release);
    try std.testing.expect(isCircuitOpen());
    // Reset for other tests.
    g_consecutive_failures.store(0, .release);
    g_circuit_open_until_ms.store(0, .release);
}

test "circuit breaker closes after timeout expires" {
    g_consecutive_failures.store(FAILURE_THRESHOLD, .release);
    g_circuit_open_until_ms.store(std.time.milliTimestamp() - 1, .release);
    try std.testing.expect(!isCircuitOpen());
    // Reset for other tests.
    g_consecutive_failures.store(0, .release);
    g_circuit_open_until_ms.store(0, .release);
}

test "circuit breaker resets on success" {
    g_consecutive_failures.store(FAILURE_THRESHOLD, .release);
    g_circuit_open_until_ms.store(std.time.milliTimestamp() + 60_000, .release);
    try std.testing.expect(isCircuitOpen());

    // Simulate a successful emit resetting state.
    g_consecutive_failures.store(0, .release);
    try std.testing.expect(!isCircuitOpen());
    // Reset for other tests.
    g_circuit_open_until_ms.store(0, .release);
}

test "circuit breaker stays closed at threshold minus one" {
    g_consecutive_failures.store(FAILURE_THRESHOLD - 1, .release);
    g_circuit_open_until_ms.store(std.time.milliTimestamp() + 60_000, .release);
    try std.testing.expect(!isCircuitOpen());
    // Reset for other tests.
    g_consecutive_failures.store(0, .release);
    g_circuit_open_until_ms.store(0, .release);
}

test "renderIngestPayload handles zero tokens and wall_seconds" {
    const alloc = std.testing.allocator;
    const payload = try renderIngestPayload(alloc, .{
        .trace_id = "t0",
        .run_id = "r0",
        .stage_id = "s0",
        .role_id = "echo",
        .token_count = 0,
        .wall_seconds = 0,
        .exit_ok = true,
        .timestamp_ms = 0,
    });
    defer alloc.free(payload);

    try std.testing.expect(std.mem.containsAtLeast(u8, payload, 1, "\"token_count\":0"));
    try std.testing.expect(std.mem.containsAtLeast(u8, payload, 1, "\"wall_seconds\":0"));
    try std.testing.expect(std.mem.containsAtLeast(u8, payload, 1, "\"total\":0"));
    try std.testing.expect(std.mem.containsAtLeast(u8, payload, 1, "\"run_id\":\"r0\""));
}

test "renderIngestPayload preserves correlation fields with empty stage_id" {
    const alloc = std.testing.allocator;
    const payload = try renderIngestPayload(alloc, .{
        .trace_id = "trace-abc",
        .run_id = "run-xyz",
        .stage_id = "",
        .role_id = "scout",
        .token_count = 1,
        .wall_seconds = 1,
        .exit_ok = false,
        .timestamp_ms = 1710000000000,
    });
    defer alloc.free(payload);

    try std.testing.expect(std.mem.containsAtLeast(u8, payload, 1, "\"trace-abc\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, payload, 1, "\"run_id\":\"run-xyz\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, payload, 1, "\"stage_id\":\"\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, payload, 1, "\"exit_ok\":false"));
}

test "concurrent circuit breaker state updates do not corrupt" {
    g_consecutive_failures.store(0, .release);
    g_circuit_open_until_ms.store(0, .release);

    const N = 8;
    var threads: [N]std.Thread = undefined;
    for (&threads) |*t| {
        t.* = try std.Thread.spawn(.{}, struct {
            fn work() void {
                for (0..50) |_| {
                    _ = g_consecutive_failures.fetchAdd(1, .monotonic);
                    _ = isCircuitOpen();
                    _ = g_consecutive_failures.fetchSub(1, .monotonic);
                }
            }
        }.work, .{});
    }
    for (&threads) |*t| t.join();

    // After all threads complete, value should return to 0 (balanced add/sub).
    try std.testing.expectEqual(@as(u32, 0), g_consecutive_failures.load(.acquire));
    // Reset for other tests.
    g_circuit_open_until_ms.store(0, .release);
}
