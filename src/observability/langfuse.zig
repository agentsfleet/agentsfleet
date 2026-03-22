//! Langfuse LLM/agent tracing integration.
//! Fire-and-forget: HTTP POST to Langfuse ingest API after each agent call.
//! Optional: configured via LANGFUSE_HOST, LANGFUSE_PUBLIC_KEY, LANGFUSE_SECRET_KEY.

const std = @import("std");
const trace = @import("trace.zig");
const obs_log = @import("logging.zig");
const metrics = @import("metrics.zig");
const backoff = @import("../reliability/backoff.zig");
const log = std.log.scoped(.langfuse);

const INGEST_PATH = "/api/public/ingestion";
pub const ERR_LANGFUSE_EXPORT_FAILED = "UZ-OBS-LANGFUSE-001";
pub const ERR_LANGFUSE_QUEUE_DROPPED = "UZ-OBS-LANGFUSE-002";

const QUEUE_CAPACITY: usize = 1024;
const MAX_RETRIES: u32 = 3;
const RETRY_BASE_MS: u64 = 250;
const RETRY_MAX_MS: u64 = 5_000;
const TRACE_ID_MAX: usize = 80;
const RUN_ID_MAX: usize = 80;
const STAGE_ID_MAX: usize = 64;
const ROLE_ID_MAX: usize = 64;

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

const QueuedTrace = struct {
    trace_id: [TRACE_ID_MAX]u8 = undefined,
    trace_id_len: u8 = 0,
    run_id: [RUN_ID_MAX]u8 = undefined,
    run_id_len: u8 = 0,
    stage_id: [STAGE_ID_MAX]u8 = undefined,
    stage_id_len: u8 = 0,
    role_id: [ROLE_ID_MAX]u8 = undefined,
    role_id_len: u8 = 0,
    token_count: u64 = 0,
    wall_seconds: u64 = 0,
    exit_ok: bool = false,
    timestamp_ms: i64 = 0,

    fn fromTrace(t: LangfuseTrace) QueuedTrace {
        var out = QueuedTrace{
            .token_count = t.token_count,
            .wall_seconds = t.wall_seconds,
            .exit_ok = t.exit_ok,
            .timestamp_ms = t.timestamp_ms,
        };
        out.trace_id_len = @intCast(copyTrunc(&out.trace_id, t.trace_id));
        out.run_id_len = @intCast(copyTrunc(&out.run_id, t.run_id));
        out.stage_id_len = @intCast(copyTrunc(&out.stage_id, t.stage_id));
        out.role_id_len = @intCast(copyTrunc(&out.role_id, t.role_id));
        return out;
    }

    fn toTrace(self: *const QueuedTrace) LangfuseTrace {
        return .{
            .trace_id = self.trace_id[0..@as(usize, self.trace_id_len)],
            .run_id = self.run_id[0..@as(usize, self.run_id_len)],
            .stage_id = self.stage_id[0..@as(usize, self.stage_id_len)],
            .role_id = self.role_id[0..@as(usize, self.role_id_len)],
            .token_count = self.token_count,
            .wall_seconds = self.wall_seconds,
            .exit_ok = self.exit_ok,
            .timestamp_ms = self.timestamp_ms,
        };
    }
};

const AsyncExporter = struct {
    alloc: std.mem.Allocator,
    cfg: LangfuseConfig,
    mutex: std.Thread.Mutex = .{},
    cond: std.Thread.Condition = .{},
    running: std.atomic.Value(bool) = std.atomic.Value(bool).init(true),
    queue: [QUEUE_CAPACITY]QueuedTrace = undefined,
    head: usize = 0,
    tail: usize = 0,
    len: usize = 0,
    thread: ?std.Thread = null,

    fn start(alloc: std.mem.Allocator, cfg: LangfuseConfig) !*AsyncExporter {
        const self = try alloc.create(AsyncExporter);
        self.* = .{
            .alloc = alloc,
            .cfg = cfg,
        };
        metrics.setLangfuseQueueDepth(0);
        self.thread = try std.Thread.spawn(.{}, runThread, .{self});
        return self;
    }

    fn stop(self: *AsyncExporter) void {
        self.running.store(false, .release);
        self.cond.broadcast();
        if (self.thread) |*t| t.join();
        metrics.setLangfuseQueueDepth(0);
    }

    fn deinit(self: *AsyncExporter) void {
        self.alloc.free(self.cfg.host);
        self.alloc.free(self.cfg.public_key);
        self.alloc.free(self.cfg.secret_key);
        self.alloc.destroy(self);
    }

    fn enqueue(self: *AsyncExporter, t: LangfuseTrace) void {
        self.mutex.lock();
        defer self.mutex.unlock();
        if (self.len >= QUEUE_CAPACITY) {
            metrics.incLangfuseDropped();
            log.warn("langfuse.queue_dropped error_code={s} trace_id={s} run_id={s} queue_depth={d}", .{ ERR_LANGFUSE_QUEUE_DROPPED, t.trace_id, t.run_id, self.len });
            return;
        }
        self.queue[self.tail] = QueuedTrace.fromTrace(t);
        self.tail = (self.tail + 1) % QUEUE_CAPACITY;
        self.len += 1;
        metrics.incLangfuseEnqueued();
        metrics.setLangfuseQueueDepth(self.len);
        self.cond.signal();
    }

    fn popLocked(self: *AsyncExporter) QueuedTrace {
        const item = self.queue[self.head];
        self.head = (self.head + 1) % QUEUE_CAPACITY;
        self.len -= 1;
        metrics.setLangfuseQueueDepth(self.len);
        return item;
    }

    fn run(self: *AsyncExporter) void {
        while (true) {
            self.mutex.lock();
            while (self.len == 0 and self.running.load(.acquire)) {
                self.cond.wait(&self.mutex);
            }
            if (self.len == 0 and !self.running.load(.acquire)) {
                self.mutex.unlock();
                break;
            }
            const item = self.popLocked();
            self.mutex.unlock();
            self.exportWithRetry(item);
        }
    }

    fn exportWithRetry(self: *AsyncExporter, item: QueuedTrace) void {
        const t = item.toTrace();
        var attempt: u32 = 0;
        while (attempt <= MAX_RETRIES) : (attempt += 1) {
            const started_ms = std.time.milliTimestamp();
            if (emitTraceInner(self.alloc, self.cfg, t)) {
                const elapsed = std.time.milliTimestamp() - started_ms;
                metrics.incLangfuseExportOk();
                metrics.addLangfuseExportLatencyMs(@as(u64, @intCast(@max(elapsed, 0))));
                metrics.setLangfuseLastSuccessAtMs(std.time.milliTimestamp());
                return;
            } else |err| {
                if (attempt < MAX_RETRIES) {
                    metrics.incLangfuseExportRetry();
                    const wait_ms = backoff.expBackoffJitter(attempt, RETRY_BASE_MS, RETRY_MAX_MS);
                    std.Thread.sleep(wait_ms * std.time.ns_per_ms);
                    continue;
                }
                metrics.incLangfuseExportFailed();
                metrics.setLangfuseLastFailureAtMs(std.time.milliTimestamp());
                obs_log.logWarnErr(.langfuse, err, "langfuse.async_emit_fail error_code={s} trace_id={s} run_id={s} host={s}", .{ ERR_LANGFUSE_EXPORT_FAILED, t.trace_id, t.run_id, self.cfg.host });
                return;
            }
        }
    }
};

fn runThread(exporter: *AsyncExporter) void {
    exporter.run();
}

var g_async_exporter = std.atomic.Value(?*AsyncExporter).init(null);

pub fn installAsyncExporter(alloc: std.mem.Allocator, cfg: LangfuseConfig) !void {
    const exporter = try AsyncExporter.start(alloc, cfg);
    const prev = g_async_exporter.swap(exporter, .acq_rel);
    if (prev) |existing| {
        existing.stop();
        existing.deinit();
    }
}

pub fn uninstallAsyncExporter() void {
    const exporter = g_async_exporter.swap(null, .acq_rel) orelse return;
    exporter.stop();
    exporter.deinit();
}

pub fn isAsyncExporterInstalled() bool {
    return g_async_exporter.load(.acquire) != null;
}

/// Queue a trace for async export when the worker exporter is installed.
/// Falls back to best-effort synchronous emit when async exporter is not installed.
pub fn emitTraceAsyncOrFallback(alloc: std.mem.Allocator, t: LangfuseTrace) void {
    if (g_async_exporter.load(.acquire)) |exporter| {
        exporter.enqueue(t);
        return;
    }
    const cfg = configFromEnv(alloc) orelse return;
    defer {
        alloc.free(cfg.host);
        alloc.free(cfg.public_key);
        alloc.free(cfg.secret_key);
    }
    emitTrace(alloc, cfg, t);
}

fn copyTrunc(dst_ptr: anytype, src: []const u8) usize {
    const dst: []u8 = dst_ptr[0..];
    const n = @min(dst.len, src.len);
    @memcpy(dst[0..n], src[0..n]);
    return n;
}

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

/// Fire-and-forget emit of a Langfuse trace. Errors are logged, never propagated.
pub fn emitTrace(alloc: std.mem.Allocator, cfg: LangfuseConfig, t: LangfuseTrace) void {
    emitTraceInner(alloc, cfg, t) catch |err| {
        obs_log.logWarnErr(.langfuse, err, "langfuse.emit_fail error_code={s} trace_id={s} run_id={s} host={s}", .{ ERR_LANGFUSE_EXPORT_FAILED, t.trace_id, t.run_id, cfg.host });
    };
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
    log.info("langfuse.emit_ok url={s} trace_id={s} payload_len={d}", .{ url, t.trace_id, payload.len });
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

test "QueuedTrace truncates oversized identifiers safely" {
    var trace_id_buf: [TRACE_ID_MAX + 10]u8 = undefined;
    @memset(&trace_id_buf, 't');
    var run_id_buf: [RUN_ID_MAX + 10]u8 = undefined;
    @memset(&run_id_buf, 'r');
    var stage_id_buf: [STAGE_ID_MAX + 10]u8 = undefined;
    @memset(&stage_id_buf, 's');
    var role_id_buf: [ROLE_ID_MAX + 10]u8 = undefined;
    @memset(&role_id_buf, 'o');

    const q = QueuedTrace.fromTrace(.{
        .trace_id = trace_id_buf[0..],
        .run_id = run_id_buf[0..],
        .stage_id = stage_id_buf[0..],
        .role_id = role_id_buf[0..],
        .token_count = 1,
        .wall_seconds = 2,
        .exit_ok = true,
        .timestamp_ms = 3,
    });
    const t = q.toTrace();

    try std.testing.expectEqual(@as(usize, TRACE_ID_MAX), t.trace_id.len);
    try std.testing.expectEqual(@as(usize, RUN_ID_MAX), t.run_id.len);
    try std.testing.expectEqual(@as(usize, STAGE_ID_MAX), t.stage_id.len);
    try std.testing.expectEqual(@as(usize, ROLE_ID_MAX), t.role_id.len);
    try std.testing.expectEqual(@as(u64, 1), t.token_count);
    try std.testing.expectEqual(@as(u64, 2), t.wall_seconds);
    try std.testing.expectEqual(true, t.exit_ok);
    try std.testing.expectEqual(@as(i64, 3), t.timestamp_ms);
}

test "async exporter install and uninstall toggles global state" {
    const alloc = std.testing.allocator;
    const host = try alloc.dupe(u8, "http://127.0.0.1:1");
    const public_key = try alloc.dupe(u8, "pk-test");
    const secret_key = try alloc.dupe(u8, "sk-test");
    try installAsyncExporter(alloc, .{
        .host = host,
        .public_key = public_key,
        .secret_key = secret_key,
    });
    try std.testing.expect(isAsyncExporterInstalled());
    uninstallAsyncExporter();
    try std.testing.expect(!isAsyncExporterInstalled());
}
