//! Accumulates Prometheus histogram lines into OTLP histogram data points.
//!
//! Prometheus histogram families render as multiple lines sharing a base name:
//!   <base>_bucket{le="0.005"} 0
//!   <base>_bucket{le="+Inf"} 5
//!   <base>_sum 1.23
//!   <base>_count 5
//!
//! This helper collects those into a single OTLP histogram data point per family,
//! converting Prometheus cumulative bucket counts into OTLP delta counts.

const std = @import("std");

const otel_json = @import("otel_json.zig");

const log = std.log.scoped(.otel_histogram);

pub const Bucket = struct {
    upper_bound: f64, // +inf stored as std.math.inf(f64)
    cumulative_count: u64,
};

pub const Histogram = struct {
    name: []u8,
    buckets: std.ArrayList(Bucket),
    sum: f64,
    count: u64,

    pub fn deinit(self: *Histogram, alloc: std.mem.Allocator) void {
        alloc.free(self.name);
        self.buckets.deinit(alloc);
    }
};

pub const Accumulator = struct {
    alloc: std.mem.Allocator,
    histograms: std.StringHashMap(*Histogram),

    pub fn init(alloc: std.mem.Allocator) Accumulator {
        return .{
            .alloc = alloc,
            .histograms = std.StringHashMap(*Histogram).init(alloc),
        };
    }

    pub fn deinit(self: *Accumulator) void {
        var it = self.histograms.iterator();
        while (it.next()) |e| {
            e.value_ptr.*.deinit(self.alloc);
            self.alloc.destroy(e.value_ptr.*);
        }
        self.histograms.deinit();
    }

    pub fn getOrCreate(self: *Accumulator, base: []const u8) !*Histogram {
        if (self.histograms.get(base)) |h| return h;
        const name_copy = try self.alloc.dupe(u8, base);
        errdefer self.alloc.free(name_copy);
        const h = try self.alloc.create(Histogram);
        errdefer self.alloc.destroy(h);
        h.* = .{
            .name = name_copy,
            .buckets = .{},
            .sum = 0,
            .count = 0,
        };
        try self.histograms.put(name_copy, h);
        return h;
    }

    /// Returns true when `line` was consumed as a histogram component.
    ///
    /// Matchers use `lastIndexOf` so a histogram whose base name itself
    /// contains `_sum`, `_count`, or `_bucket` (e.g. `foo_count_seconds`)
    /// parses correctly — the real suffix is always the rightmost occurrence
    /// on a valid Prometheus exposition line.
    ///
    /// Labeled histograms (any label beyond `le` on bucket/sum/count lines)
    /// are rejected: the accumulator keys on base name only, so accepting
    /// them would conflate multiple time-series under one OTLP histogram.
    /// Returns false + warn-log so format drift surfaces in operator logs.
    pub fn tryIngest(self: *Accumulator, line: []const u8) !bool {
        if (std.mem.lastIndexOf(u8, line, "_bucket{")) |b_idx| {
            return self.ingestBucket(line, b_idx);
        }
        if (std.mem.lastIndexOf(u8, line, "_sum")) |s_idx| {
            if (splitSuffix(line, s_idx, "_sum")) |parts| {
                if (parts.has_labels) {
                    log.warn("otel_histogram.labeled_sum_rejected base={s}", .{parts.base});
                    return false;
                }
                const h = try self.getOrCreate(parts.base);
                h.sum = std.fmt.parseFloat(f64, parts.value) catch |err| blk: {
                    log.warn("otel_histogram.sum_parse_failed base={s} value={s} err={}", .{ parts.base, parts.value, err });
                    break :blk 0;
                };
                return true;
            }
        }
        if (std.mem.lastIndexOf(u8, line, "_count")) |c_idx| {
            if (splitSuffix(line, c_idx, "_count")) |parts| {
                if (parts.has_labels) {
                    log.warn("otel_histogram.labeled_count_rejected base={s}", .{parts.base});
                    return false;
                }
                const h = try self.getOrCreate(parts.base);
                h.count = std.fmt.parseInt(u64, parts.value, 10) catch |err| blk: {
                    log.warn("otel_histogram.count_parse_failed base={s} value={s} err={}", .{ parts.base, parts.value, err });
                    break :blk 0;
                };
                return true;
            }
        }
        return false;
    }

    fn ingestBucket(self: *Accumulator, line: []const u8, b_idx: usize) !bool {
        const base = line[0..b_idx];
        const brace_start = b_idx + "_bucket".len;
        const brace_end = std.mem.indexOfScalarPos(u8, line, brace_start, '}') orelse return false;
        const labels = line[brace_start + 1 .. brace_end];

        // Reject labeled histograms (anything beyond `le`) — the accumulator
        // is keyed on base name, so accepting them would conflate series.
        if (hasLabelsBeyondLe(labels)) {
            log.warn("otel_histogram.labeled_bucket_rejected base={s}", .{base});
            return false;
        }

        const le_prefix = "le=\"";
        const le_pos = std.mem.indexOf(u8, labels, le_prefix) orelse return false;
        const le_val_start = le_pos + le_prefix.len;
        const le_val_end = std.mem.indexOfScalarPos(u8, labels, le_val_start, '"') orelse return false;
        const le_str = labels[le_val_start..le_val_end];

        const value_str = std.mem.trim(u8, line[brace_end + 1 ..], " \t\r\n");
        const cnt = std.fmt.parseInt(u64, value_str, 10) catch |err| {
            log.warn("otel_histogram.bucket_parse_failed base={s} value={s} err={}", .{ base, value_str, err });
            return false;
        };

        const upper: f64 = if (std.mem.eql(u8, le_str, "+Inf"))
            std.math.inf(f64)
        else
            std.fmt.parseFloat(f64, le_str) catch |err| {
                log.warn("otel_histogram.bucket_le_parse_failed base={s} le={s} err={}", .{ base, le_str, err });
                return false;
            };

        const h = try self.getOrCreate(base);
        try h.buckets.append(self.alloc, .{ .upper_bound = upper, .cumulative_count = cnt });
        return true;
    }

    /// Write OTLP histogram data point JSON objects for all accumulated
    /// histograms. `first_ptr` tracks whether a preceding metric has been
    /// written (so the caller can manage commas in a single `metrics` array).
    ///
    /// Prometheus histograms are cumulative-from-process-start (counts only
    /// grow, never reset between scrapes), so the correct OTLP temporality is
    /// CUMULATIVE with a stable `startTimeUnixNano` anchored at process start.
    /// Per-bucket counts are computed by differencing consecutive cumulative
    /// bucket counts — this is the Prometheus-cumulative-across-BUCKETS → OTLP
    /// per-bucket conversion, which is orthogonal to temporality (which is
    /// about cumulative-across-TIME).
    pub fn writeOtlp(self: *Accumulator, writer: anytype, start_ns: u64, now_ns: u64, first_ptr: *bool) !void {
        var it = self.histograms.iterator();
        while (it.next()) |e| {
            const h = e.value_ptr.*;
            if (h.buckets.items.len == 0) continue;

            std.mem.sort(Bucket, h.buckets.items, {}, bucketLess);

            if (!first_ptr.*) try writer.writeAll(",");
            first_ptr.* = false;

            try writer.print("{{\"name\":\"{s}\",\"histogram\":{{\"aggregationTemporality\":2,\"dataPoints\":[{{", .{h.name});
            try writer.writeAll("\"explicitBounds\":[");
            var wrote_bound = false;
            for (h.buckets.items) |b| {
                // OTLP excludes the +Inf bound; non-finite values (nan/-inf)
                // would also produce invalid JSON via `{d}` — skip defensively.
                if (!std.math.isFinite(b.upper_bound)) continue;
                if (wrote_bound) try writer.writeAll(",");
                wrote_bound = true;
                try writer.print("{d}", .{b.upper_bound});
            }
            try writer.writeAll("],\"bucketCounts\":[");
            var prev: u64 = 0;
            for (h.buckets.items, 0..) |b, i| {
                if (i > 0) try writer.writeAll(",");
                const delta = if (b.cumulative_count >= prev) b.cumulative_count - prev else 0;
                try writer.print("\"{d}\"", .{delta});
                prev = b.cumulative_count;
            }
            // Non-finite sum (nan/inf) would serialize as unquoted `nan`/`inf`
            // — invalid JSON. Clamp to 0; the `count` field still carries the
            // sample count so the data point isn't entirely lost.
            const sum_emit: f64 = if (std.math.isFinite(h.sum)) h.sum else 0;
            try writer.print("],\"count\":\"{d}\",\"sum\":{d},\"startTimeUnixNano\":\"{d}\",\"timeUnixNano\":\"{d}\"}}]}}}}", .{ h.count, sum_emit, start_ns, now_ns });
        }
    }
};

fn bucketLess(_: void, a: Bucket, b: Bucket) bool {
    return a.upper_bound < b.upper_bound;
}

const Suffix = struct { base: []const u8, value: []const u8, has_labels: bool };

/// Split a Prometheus histogram sum/count line at `suffix`. Returns null when
/// the character after the suffix is neither `{` (labeled) nor a whitespace
/// separator (unlabeled) — i.e. the match was spurious substring.
///
/// Degenerate input `foo_sum{} 1.23` (empty braces) is accepted and treated
/// as unlabeled; standard Prometheus renderers omit the braces entirely in
/// that case, so the branch never fires in practice.
fn splitSuffix(line: []const u8, suffix_idx: usize, suffix: []const u8) ?Suffix {
    const after = suffix_idx + suffix.len;
    if (after >= line.len) return null;
    const base = line[0..suffix_idx];
    if (line[after] == '{') {
        const brace_end = std.mem.indexOfScalarPos(u8, line, after + 1, '}') orelse return null;
        if (brace_end + 1 >= line.len) return null;
        const value_start = brace_end + 1;
        return .{
            .base = base,
            .value = std.mem.trim(u8, line[value_start..], " \t\r\n"),
            .has_labels = (brace_end > after + 1),
        };
    }
    if (line[after] == ' ' or line[after] == '\t') {
        return .{
            .base = base,
            .value = std.mem.trim(u8, line[after + 1 ..], " \t\r\n"),
            .has_labels = false,
        };
    }
    return null;
}

/// True when the label set contains any key other than `le`. Used to reject
/// labeled histograms whose base name alone can't disambiguate series.
/// Uses `findUnescapedQuote` so a label value containing a Prometheus-
/// escaped quote (`\"`) doesn't falsely terminate the scan mid-value.
fn hasLabelsBeyondLe(labels: []const u8) bool {
    var rest = labels;
    while (rest.len > 0) {
        const eq = std.mem.indexOfScalar(u8, rest, '=') orelse return false;
        const key = std.mem.trim(u8, rest[0..eq], " ,");
        if (!std.mem.eql(u8, key, "le")) return true;
        if (eq + 1 >= rest.len or rest[eq + 1] != '"') return false;
        const q2 = otel_json.findUnescapedQuote(rest, eq + 2) orelse return false;
        rest = rest[q2 + 1 ..];
        if (rest.len > 0 and rest[0] == ',') rest = rest[1..];
    }
    return false;
}

test {
    _ = @import("otel_histogram_test.zig");
}
