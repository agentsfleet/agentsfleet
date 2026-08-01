//! Deterministic corpus generation: a seed manifest expands into synthetic
//! telemetry (JSON-lines) whose bytes — and therefore whose hash — depend only
//! on the manifest. No wall clock and no ambient randomness ever enter the
//! render path, which is what makes the corpus hash a reproducibility proof:
//! identical manifest, identical corpus, identical hash.

const std = @import("std");
const manifest = @import("manifest.zig");

const Sha256 = std.crypto.hash.sha2.Sha256;

/// Telemetry sampling cadence inside a seed window.
const SAMPLE_INTERVAL_MS: i64 = 5_000;
/// Hard cap on documents a single seed may expand to; bounds corpus size
/// regardless of what a manifest declares.
const MAX_DOCS_PER_SEED: usize = 2_000;
/// Quiet-window signal floor the injected magnitudes sit on top of.
const BASELINE_ERROR_RATE_PCT: u32 = 1;
const BASELINE_LATENCY_MS: u32 = 120;
/// Deterministic jitter band applied per document so the corpus looks like
/// telemetry rather than a flat line. Sourced from a seed-id-keyed PRNG.
const JITTER_LATENCY_MS: u32 = 40;

pub const CORPUS_HASH_HEX_LEN = Sha256.digest_length * 2;
const LINE_BUF_LEN = 320;

/// Render every corpus line into `sink` (anything with `consume([]const u8)`).
/// Lines are built in a stack buffer — the render loop allocates nothing.
fn emitCorpus(m: manifest.SeedManifest, sink: anytype) !void {
    for (m.seeds) |seed| {
        var prng = std.Random.DefaultPrng.init(seedKey(m.set, seed.id));
        const random = prng.random();
        const doc_count = docCount(seed.duration_ms);
        var line_buf: [LINE_BUF_LEN]u8 = undefined;
        for (0..doc_count) |i| {
            const ts_ms = m.epoch_ms + seed.offset_ms + @as(i64, @intCast(i)) * SAMPLE_INTERVAL_MS;
            const error_rate = if (seed.clean)
                BASELINE_ERROR_RATE_PCT
            else
                BASELINE_ERROR_RATE_PCT + seed.magnitude_pct;
            const latency = BASELINE_LATENCY_MS +
                random.uintLessThan(u32, JITTER_LATENCY_MS) +
                if (seed.clean) 0 else seed.magnitude_pct;
            const line = try std.fmt.bufPrint(&line_buf, "{{\"ts\":{d},\"service\":\"{s}\",\"class\":\"{s}\",\"seed\":\"{s}\",\"error_rate_pct\":{d},\"latency_ms\":{d}}}\n", .{
                ts_ms, seed.service, @tagName(seed.class), seed.id, error_rate, latency,
            });
            try sink.consume(line);
        }
    }
}

fn docCount(duration_ms: i64) usize {
    const by_interval: usize = @intCast(@max(1, @divTrunc(duration_ms, SAMPLE_INTERVAL_MS)));
    return @min(by_interval, MAX_DOCS_PER_SEED);
}

/// PRNG key derived from the split + seed id only — deterministic across runs
/// and hosts, distinct across seeds.
fn seedKey(set: manifest.SetKind, id: []const u8) u64 {
    var h = Sha256.init(.{});
    h.update(@tagName(set));
    h.update(id);
    var digest: [Sha256.digest_length]u8 = undefined;
    h.final(&digest);
    return std.mem.readInt(u64, digest[0..8], .big);
}

const HashSink = struct {
    hasher: *Sha256,

    fn consume(self: HashSink, line: []const u8) !void {
        self.hasher.update(line);
    }
};

const FileSink = struct {
    file: std.Io.File,
    io: std.Io,

    fn consume(self: FileSink, line: []const u8) !void {
        try self.file.writeStreamingAll(self.io, line);
    }
};

/// The reproducibility proof: SHA-256 over the rendered corpus bytes, hex.
pub fn corpusHashHex(m: manifest.SeedManifest) ![CORPUS_HASH_HEX_LEN]u8 {
    var hasher = Sha256.init(.{});
    try emitCorpus(m, HashSink{ .hasher = &hasher });
    var digest: [Sha256.digest_length]u8 = undefined;
    hasher.final(&digest);
    return std.fmt.bytesToHex(digest, .lower);
}

/// Write the rendered corpus to `path` (truncates). The demo playbook ships
/// this file into Elastic; tests and the hash never need it on disk.
pub fn writeCorpus(io: std.Io, m: manifest.SeedManifest, path: []const u8) !void {
    const file = try std.Io.Dir.cwd().createFile(io, path, .{ .truncate = true });
    defer file.close(io);
    try emitCorpus(m, FileSink{ .file = file, .io = io });
}
