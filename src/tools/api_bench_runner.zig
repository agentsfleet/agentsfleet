const std = @import("std");
const builtin = @import("builtin");
const zbench = @import("zbench");

const Mode = enum { bench, soak, profile };

const Defaults = struct {
    duration_sec: u64,
    concurrency: usize,
    max_error_rate: f64,
    max_p95_ms: f64,
    max_rss_growth_mb: f64,
};

const Config = struct {
    mode: Mode,
    url: []const u8,
    method: std.http.Method,
    bearer_token: ?[]const u8,
    request_body: ?[]const u8,
    content_type: ?[]const u8,
    duration_sec: u64,
    concurrency: usize,
    timeout_ms: u64,
    max_error_rate: f64,
    max_p95_ms: f64,
    max_rss_growth_mb: f64,
};

const Summary = struct {
    mode: []const u8,
    config: struct {
        url: []const u8,
        method: []const u8,
        has_auth_bearer: bool,
        has_request_body: bool,
        content_type: ?[]const u8,
        duration_sec: u64,
        concurrency: usize,
        timeout_ms: u64,
        max_error_rate: f64,
        max_p95_ms: f64,
        max_rss_growth_mb: f64,
    },
    counts: struct {
        total: u64,
        ok: u64,
        fail: u64,
        timeout: u64,
    },
    latency_ms: struct {
        p50: f64,
        p95: f64,
        p99: f64,
        max: f64,
    },
    rates: struct {
        req_per_sec: f64,
        error_rate: f64,
    },
    memory_rss_mb: struct {
        start: ?f64,
        end: ?f64,
        growth: ?f64,
    },
    started_at_unix_ms: i64,
    ended_at_unix_ms: i64,
    passed: bool,
};

const Shared = struct {
    alloc: std.mem.Allocator,
    mutex: std.Thread.Mutex = .{},
    total: u64 = 0,
    ok: u64 = 0,
    fail: u64 = 0,
    timeout: u64 = 0,
    latencies_ns: std.ArrayListUnmanaged(u64) = .{},

    fn init(alloc: std.mem.Allocator) Shared {
        return .{ .alloc = alloc };
    }

    fn deinit(self: *Shared) void {
        self.latencies_ns.deinit(self.alloc);
    }

    fn record(self: *Shared, success: bool, latency_ns: u64) void {
        self.mutex.lock();
        defer self.mutex.unlock();

        self.total += 1;
        if (success) {
            self.ok += 1;
        } else {
            self.fail += 1;
        }

        self.latencies_ns.append(self.alloc, latency_ns) catch {};
    }
};

const WorkerCtx = struct {
    cfg: *const Config,
    shared: *Shared,
    deadline_ns: i128,
};

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const alloc = gpa.allocator();

    const cfg = try readConfig(alloc);
    defer alloc.free(cfg.url);
    if (cfg.bearer_token) |token| alloc.free(token);
    if (cfg.request_body) |body| alloc.free(body);
    if (cfg.content_type) |content_type| alloc.free(content_type);

    // Pre-flight: verify the target server is reachable before starting the benchmark.
    {
        var client: std.http.Client = .{ .allocator = alloc };
        defer client.deinit();
        var body: std.ArrayList(u8) = .{};
        defer body.deinit(alloc);
        var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &body);
        _ = performFetch(alloc, &client, &cfg, &aw.writer) catch {
            std.debug.print("error: server not reachable at {s}\n", .{cfg.url});
            std.debug.print("hint: start the server first (e.g. `make up`) then re-run `make bench`\n", .{});
            std.process.exit(1);
        };
    }

    const rss_start = try readRssBytes(alloc);
    const started_unix_ms = std.time.milliTimestamp();
    const start_ns = std.time.nanoTimestamp();
    const duration_ns: i128 = @as(i128, cfg.duration_sec) * std.time.ns_per_s;
    const deadline_ns = start_ns + duration_ns;

    var shared = Shared.init(alloc);
    defer shared.deinit();

    const threads = try alloc.alloc(std.Thread, cfg.concurrency);
    defer alloc.free(threads);

    var worker_ctx = WorkerCtx{ .cfg = &cfg, .shared = &shared, .deadline_ns = deadline_ns };

    for (threads) |*thread| {
        thread.* = try std.Thread.spawn(.{}, workerMain, .{&worker_ctx});
    }
    for (threads) |thread| {
        thread.join();
    }

    const ended_unix_ms = std.time.milliTimestamp();
    const rss_end = try readRssBytes(alloc);

    const total = shared.total;
    const fail = shared.fail;
    const error_rate = if (total > 0) @as(f64, @floatFromInt(fail)) / @as(f64, @floatFromInt(total)) else 1.0;

    var p50: f64 = 0;
    var p95: f64 = 0;
    var p99: f64 = 0;
    var max_latency_ms: f64 = 0;

    if (shared.latencies_ns.items.len > 0) {
        const latency_stats = try zbench.statistics.Statistics(u64).init(alloc, shared.latencies_ns.items);
        const sorted_latencies = try alloc.dupe(u64, shared.latencies_ns.items);
        defer alloc.free(sorted_latencies);
        std.sort.heap(u64, sorted_latencies, {}, std.sort.asc(u64));

        p50 = nsToMs(percentileFromSortedNs(sorted_latencies, 50.0));
        p95 = nsToMs(percentileFromSortedNs(sorted_latencies, 95.0));
        p99 = nsToMs(percentileFromSortedNs(sorted_latencies, 99.0));
        max_latency_ms = nsToMs(latency_stats.max);
    }

    const duration_f: f64 = @floatFromInt(@max(cfg.duration_sec, 1));
    const req_per_sec = @as(f64, @floatFromInt(total)) / duration_f;

    const rss_growth_mb: ?f64 = if (rss_start != null and rss_end != null)
        (@as(f64, @floatFromInt(@as(i128, @intCast(rss_end.?)) - @as(i128, @intCast(rss_start.?)))) / (1024.0 * 1024.0))
    else
        null;

    const pass_rss = if (rss_growth_mb) |g| g <= cfg.max_rss_growth_mb else true;
    const passed = error_rate <= cfg.max_error_rate and p95 <= cfg.max_p95_ms and pass_rss;

    const summary: Summary = .{
        .mode = @tagName(cfg.mode),
        .config = .{
            .url = cfg.url,
            .method = @tagName(cfg.method),
            .has_auth_bearer = cfg.bearer_token != null,
            .has_request_body = cfg.request_body != null,
            .content_type = cfg.content_type,
            .duration_sec = cfg.duration_sec,
            .concurrency = cfg.concurrency,
            .timeout_ms = cfg.timeout_ms,
            .max_error_rate = cfg.max_error_rate,
            .max_p95_ms = cfg.max_p95_ms,
            .max_rss_growth_mb = cfg.max_rss_growth_mb,
        },
        .counts = .{
            .total = total,
            .ok = shared.ok,
            .fail = shared.fail,
            .timeout = shared.timeout,
        },
        .latency_ms = .{
            .p50 = round2(p50),
            .p95 = round2(p95),
            .p99 = round2(p99),
            .max = round2(max_latency_ms),
        },
        .rates = .{
            .req_per_sec = round2(req_per_sec),
            .error_rate = round6(error_rate),
        },
        .memory_rss_mb = .{
            .start = if (rss_start) |v| round2(@as(f64, @floatFromInt(v)) / (1024.0 * 1024.0)) else null,
            .end = if (rss_end) |v| round2(@as(f64, @floatFromInt(v)) / (1024.0 * 1024.0)) else null,
            .growth = if (rss_growth_mb) |v| round2(v) else null,
        },
        .started_at_unix_ms = started_unix_ms,
        .ended_at_unix_ms = ended_unix_ms,
        .passed = passed,
    };

    try std.fs.cwd().makePath(".tmp");
    const stamp_ms: u64 = @intCast(@max(started_unix_ms, 0));
    const out_path = try std.fmt.allocPrint(alloc, ".tmp/api-bench-{s}-{d}.json", .{ @tagName(cfg.mode), stamp_ms });
    defer alloc.free(out_path);

    var json_buf: std.ArrayList(u8) = .{};
    defer json_buf.deinit(alloc);
    var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &json_buf);
    try std.json.Stringify.value(summary, .{ .whitespace = .indent_2 }, &aw.writer);
    try aw.writer.writeByte('\n');
    const encoded = try aw.toOwnedSlice();
    defer alloc.free(encoded);

    var f = try std.fs.cwd().createFile(out_path, .{});
    defer f.close();
    try f.writeAll(encoded);

    if (cfg.mode == .profile) {
        const timeline_path = try std.fmt.allocPrint(alloc, ".tmp/api-bench-profile-timeline-{d}.json", .{stamp_ms});
        defer alloc.free(timeline_path);
        var tf = try std.fs.cwd().createFile(timeline_path, .{});
        defer tf.close();
        try tf.writeAll("{\n  \"timeline\": []\n}\n");
        std.debug.print("profile_timeline={s}\n", .{timeline_path});
    }

    std.debug.print("mode={s} url={s} duration={d}s concurrency={d}\n", .{ @tagName(cfg.mode), cfg.url, cfg.duration_sec, cfg.concurrency });
    std.debug.print("total={d} ok={d} fail={d} timeout={d}\n", .{ summary.counts.total, summary.counts.ok, summary.counts.fail, summary.counts.timeout });
    std.debug.print("latency_ms p50={d:.2} p95={d:.2} p99={d:.2} max={d:.2}\n", .{ summary.latency_ms.p50, summary.latency_ms.p95, summary.latency_ms.p99, summary.latency_ms.max });
    std.debug.print("req_per_sec={d:.2} error_rate={d:.6}\n", .{ summary.rates.req_per_sec, summary.rates.error_rate });
    if (summary.memory_rss_mb.growth) |growth| {
        std.debug.print("rss_mb start={d:.2} end={d:.2} growth={d:.2} limit={d:.2}\n", .{ summary.memory_rss_mb.start.?, summary.memory_rss_mb.end.?, growth, cfg.max_rss_growth_mb });
    } else {
        std.debug.print("rss_mb start=n/a end=n/a growth=n/a limit={d:.2}\n", .{cfg.max_rss_growth_mb});
    }
    std.debug.print("artifact={s}\n", .{out_path});

    if (!passed) {
        if (summary.memory_rss_mb.growth) |growth| {
            std.debug.print("error: bench gate failed (error_rate={d:.6} > {d:.6} or p95={d:.2}ms > {d:.2}ms or rss_growth={d:.2}MB > {d:.2}MB)\n", .{ summary.rates.error_rate, cfg.max_error_rate, summary.latency_ms.p95, cfg.max_p95_ms, growth, cfg.max_rss_growth_mb });
        } else {
            std.debug.print("error: bench gate failed (error_rate={d:.6} > {d:.6} or p95={d:.2}ms > {d:.2}ms)\n", .{ summary.rates.error_rate, cfg.max_error_rate, summary.latency_ms.p95, cfg.max_p95_ms });
        }
        std.process.exit(1);
    }
}

fn workerMain(ctx: *WorkerCtx) void {
    var client: std.http.Client = .{ .allocator = std.heap.page_allocator };
    defer client.deinit();

    var body: std.ArrayList(u8) = .{};
    defer body.deinit(std.heap.page_allocator);

    while (std.time.nanoTimestamp() < ctx.deadline_ns) {
        body.clearRetainingCapacity();
        var aw: std.Io.Writer.Allocating = .fromArrayList(std.heap.page_allocator, &body);
        const start_ns = std.time.nanoTimestamp();

        const result = performFetch(std.heap.page_allocator, &client, ctx.cfg, &aw.writer) catch {
            const elapsed_ns: u64 = @intCast(@max(std.time.nanoTimestamp() - start_ns, 0));
            ctx.shared.record(false, elapsed_ns);
            continue;
        };

        const elapsed_ns: u64 = @intCast(@max(std.time.nanoTimestamp() - start_ns, 0));
        const code = @intFromEnum(result.status);
        const success = code >= 200 and code < 300;
        ctx.shared.record(success, elapsed_ns);
    }
}

fn percentileFromSortedNs(sorted_ns: []const u64, percentile: f64) u64 {
    if (sorted_ns.len == 0) return 0;
    const idx_f = std.math.ceil((percentile / 100.0) * @as(f64, @floatFromInt(sorted_ns.len)));
    const idx_u: u64 = @intFromFloat(idx_f);
    const idx: usize = @intCast(@min(idx_u, sorted_ns.len) - 1);
    return sorted_ns[idx];
}

fn readConfig(alloc: std.mem.Allocator) !Config {
    const mode = try readMode(alloc);
    const defaults = modeDefaults(mode);

    const url = try envOwnedOrDefault(alloc, "API_BENCH_URL", "http://127.0.0.1:3000/healthz");
    const method_text = try envOwnedOrDefault(alloc, "API_BENCH_METHOD", "GET");
    defer alloc.free(method_text);
    const method = parseMethod(method_text) orelse {
        std.debug.print("error: unsupported API_BENCH_METHOD={s}; expected GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS\n", .{method_text});
        std.process.exit(2);
    };
    const bearer_token = try envOwnedOrNull(alloc, "API_BENCH_AUTH_BEARER");
    const request_body = try envOwnedOrNull(alloc, "API_BENCH_BODY");
    const content_type = try envOwnedOrNull(alloc, "API_BENCH_CONTENT_TYPE");

    const duration_sec = try envU64OrDefault(alloc, "API_BENCH_DURATION_SEC", defaults.duration_sec);
    const concurrency = try envUsizeOrDefault(alloc, "API_BENCH_CONCURRENCY", defaults.concurrency);
    const timeout_ms = try envU64OrDefault(alloc, "API_BENCH_TIMEOUT_MS", 2500);
    const max_error_rate = try envF64OrDefault(alloc, "API_BENCH_MAX_ERROR_RATE", defaults.max_error_rate);
    const max_p95_ms = try envF64OrDefault(alloc, "API_BENCH_MAX_P95_MS", defaults.max_p95_ms);
    const max_rss_growth_mb = try envF64OrDefault(alloc, "API_BENCH_MAX_RSS_GROWTH_MB", defaults.max_rss_growth_mb);

    return .{
        .mode = mode,
        .url = url,
        .method = method,
        .bearer_token = bearer_token,
        .request_body = request_body,
        .content_type = content_type,
        .duration_sec = @max(duration_sec, 1),
        .concurrency = @max(concurrency, 1),
        .timeout_ms = @max(timeout_ms, 1),
        .max_error_rate = max_error_rate,
        .max_p95_ms = max_p95_ms,
        .max_rss_growth_mb = max_rss_growth_mb,
    };
}

fn readMode(alloc: std.mem.Allocator) !Mode {
    const raw = try envOwnedOrDefault(alloc, "BENCH_MODE", "bench");
    defer alloc.free(raw);
    if (std.ascii.eqlIgnoreCase(raw, "bench")) return .bench;
    if (std.ascii.eqlIgnoreCase(raw, "soak")) return .soak;
    if (std.ascii.eqlIgnoreCase(raw, "profile")) return .profile;
    std.debug.print("error: invalid BENCH_MODE={s}; expected bench|soak|profile\n", .{raw});
    std.process.exit(2);
}

fn modeDefaults(mode: Mode) Defaults {
    return switch (mode) {
        .bench => .{ .duration_sec = 20, .concurrency = 20, .max_error_rate = 0.01, .max_p95_ms = 250, .max_rss_growth_mb = 128 },
        .soak => .{ .duration_sec = 600, .concurrency = 12, .max_error_rate = 0.02, .max_p95_ms = 400, .max_rss_growth_mb = 256 },
        .profile => .{ .duration_sec = 45, .concurrency = 16, .max_error_rate = 0.02, .max_p95_ms = 300, .max_rss_growth_mb = 192 },
    };
}

fn parseMethod(raw: []const u8) ?std.http.Method {
    if (std.ascii.eqlIgnoreCase(raw, "GET")) return .GET;
    if (std.ascii.eqlIgnoreCase(raw, "POST")) return .POST;
    if (std.ascii.eqlIgnoreCase(raw, "PUT")) return .PUT;
    if (std.ascii.eqlIgnoreCase(raw, "PATCH")) return .PATCH;
    if (std.ascii.eqlIgnoreCase(raw, "DELETE")) return .DELETE;
    if (std.ascii.eqlIgnoreCase(raw, "HEAD")) return .HEAD;
    if (std.ascii.eqlIgnoreCase(raw, "OPTIONS")) return .OPTIONS;
    return null;
}

fn envOwnedOrDefault(alloc: std.mem.Allocator, key: []const u8, default_value: []const u8) ![]const u8 {
    if (std.process.getEnvVarOwned(alloc, key)) |v| {
        const trimmed = std.mem.trim(u8, v, " \t\r\n");
        if (trimmed.len == 0) {
            alloc.free(v);
            return alloc.dupe(u8, default_value);
        }
        return v;
    } else |_| {
        return alloc.dupe(u8, default_value);
    }
}

fn envOwnedOrNull(alloc: std.mem.Allocator, key: []const u8) !?[]const u8 {
    const raw = std.process.getEnvVarOwned(alloc, key) catch return null;
    if (std.mem.trim(u8, raw, " \t\r\n").len == 0) {
        alloc.free(raw);
        return null;
    }
    return raw;
}

fn performFetch(
    alloc: std.mem.Allocator,
    client: *std.http.Client,
    cfg: *const Config,
    response_writer: *std.Io.Writer,
) !std.http.Client.FetchResult {
    var auth_header: ?[]u8 = null;
    defer if (auth_header) |header| alloc.free(header);

    var headers_buf: [2]std.http.Header = undefined;
    const headers = try buildExtraHeaders(alloc, cfg, &auth_header, &headers_buf);

    return client.fetch(.{
        .location = .{ .url = cfg.url },
        .method = cfg.method,
        .payload = cfg.request_body,
        .extra_headers = headers,
        .response_writer = response_writer,
    });
}

fn buildExtraHeaders(
    alloc: std.mem.Allocator,
    cfg: *const Config,
    auth_header: *?[]u8,
    headers_buf: *[2]std.http.Header,
) ![]const std.http.Header {
    var count: usize = 0;

    if (cfg.bearer_token) |token| {
        const header = try std.fmt.allocPrint(alloc, "Bearer {s}", .{token});
        auth_header.* = header;
        headers_buf[count] = .{ .name = "authorization", .value = header };
        count += 1;
    }

    if (cfg.content_type) |content_type| {
        headers_buf[count] = .{ .name = "content-type", .value = content_type };
        count += 1;
    }

    return headers_buf[0..count];
}

fn envU64OrDefault(alloc: std.mem.Allocator, key: []const u8, default_value: u64) !u64 {
    const raw = std.process.getEnvVarOwned(alloc, key) catch return default_value;
    defer alloc.free(raw);
    const trimmed = std.mem.trim(u8, raw, " \t\r\n");
    if (trimmed.len == 0) return default_value;
    return std.fmt.parseInt(u64, trimmed, 10) catch default_value;
}

fn envUsizeOrDefault(alloc: std.mem.Allocator, key: []const u8, default_value: usize) !usize {
    const raw = std.process.getEnvVarOwned(alloc, key) catch return default_value;
    defer alloc.free(raw);
    const trimmed = std.mem.trim(u8, raw, " \t\r\n");
    if (trimmed.len == 0) return default_value;
    return std.fmt.parseInt(usize, trimmed, 10) catch default_value;
}

fn envF64OrDefault(alloc: std.mem.Allocator, key: []const u8, default_value: f64) !f64 {
    const raw = std.process.getEnvVarOwned(alloc, key) catch return default_value;
    defer alloc.free(raw);
    const trimmed = std.mem.trim(u8, raw, " \t\r\n");
    if (trimmed.len == 0) return default_value;
    return std.fmt.parseFloat(f64, trimmed) catch default_value;
}

fn readRssBytes(alloc: std.mem.Allocator) !?u64 {
    if (builtin.os.tag != .linux) return null;

    const content = std.fs.cwd().readFileAlloc(alloc, "/proc/self/status", 1024 * 1024) catch return null;
    defer alloc.free(content);

    var lines = std.mem.splitScalar(u8, content, '\n');
    while (lines.next()) |line| {
        if (!std.mem.startsWith(u8, line, "VmRSS:")) continue;
        const rest = std.mem.trim(u8, line["VmRSS:".len..], " \t");
        var toks = std.mem.tokenizeScalar(u8, rest, ' ');
        const kb_text = toks.next() orelse return null;
        const kb = std.fmt.parseInt(u64, kb_text, 10) catch return null;
        return kb * 1024;
    }
    return null;
}

fn nsToMs(v: u64) f64 {
    return @as(f64, @floatFromInt(v)) / @as(f64, std.time.ns_per_ms);
}

fn round2(v: f64) f64 {
    return std.math.round(v * 100.0) / 100.0;
}

fn round6(v: f64) f64 {
    return std.math.round(v * 1_000_000.0) / 1_000_000.0;
}
