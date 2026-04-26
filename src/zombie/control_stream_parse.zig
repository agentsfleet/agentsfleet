//! Parser helpers for control-stream field decoding.
//!
//! Extracted from `control_stream.zig` to keep that file under the 350-line
//! cap and to give the parser a stable, dedicated test surface. These are
//! pure functions — Redis-free, allocator-free where possible.

const std = @import("std");

const log = std.log.scoped(.control_stream);

/// Parse the `config_revision` field of a control-stream entry.
///
/// Returns 0 on malformed input (preserves M40 best-effort semantics) and
/// emits a `warn` log line so ops sees the corruption. Pre-greptile fix
/// this was an inline `parseInt(...) catch 0` that silently mapped `""`,
/// `"1.5"`, `"abc"` to revision 0 — invisible until M41's hot-reload
/// starts gating reload on the value.
/// Maximum bytes of the malformed `raw` value to include in the log line.
/// 32 bytes lets ops diagnose common shapes (empty, decimal, alpha, stale
/// config string) without unbounded log volume on adversarial / large
/// inputs. The outer `zombiedLog` (main.zig) JSON-escapes the full message
/// so embedded control chars / non-UTF-8 bytes don't break log parsing.
const log_preview_max_bytes: usize = 32;

pub fn parseConfigRevision(raw: []const u8) i64 {
    return std.fmt.parseInt(i64, raw, 10) catch |err| {
        const preview = raw[0..@min(raw.len, log_preview_max_bytes)];
        log.warn(
            "control.decode_revision_invalid err={s} raw_len={d} raw_preview={s}",
            .{ @errorName(err), raw.len, preview },
        );
        return 0;
    };
}
