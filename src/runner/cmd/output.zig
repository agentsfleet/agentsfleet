//! Output-as-a-service for the operator CLI (docs/CLI_DX_PILLARS.md). Handlers
//! return data + outcomes; this module owns how they reach the user: auto-JSON
//! when stdout is NOT a TTY (an LLM/script consumer — Pillar 7) or `--json` is
//! set, human text otherwise. Plain ASCII only — no ANSI, no decorative glyphs —
//! so piped output and the help golden stay byte-stable.

const std = @import("std");

pub const FLAG_JSON = "--json";

pub const Audience = enum { json, human };

/// Pick the rendering audience: JSON when forced (`--json`) or stdout is piped
/// (not a TTY), else human. `std.posix.isatty` is the stable TTY probe.
pub fn audience(force_json: bool) Audience {
    if (force_json) return .json;
    return if (std.posix.isatty(std.posix.STDOUT_FILENO)) .human else .json;
}

/// Machine-stable failure (Pillar 4): what failed (`code`), why (`message`),
/// and the actionable fix (`suggestion`). Codes are CLI-local stable strings
/// (the server's own registry codes ride through on rejection messages).
pub const CliError = struct {
    code: []const u8,
    message: []const u8,
    suggestion: []const u8,
};

/// Render a structured error to stderr in the caller's audience and return the
/// process exit code (1). JSON shape mirrors the pillars: `{"ok":false,...}`.
pub fn fail(a: Audience, alloc: std.mem.Allocator, e: CliError) u8 {
    switch (a) {
        .json => {
            const env = JsonError{ .ok = false, .@"error" = e };
            const s = std.json.Stringify.valueAlloc(alloc, env, .{}) catch {
                writeErr("{\"ok\":false}\n");
                return 1;
            };
            defer alloc.free(s);
            writeErr(s);
            writeErr("\n");
        },
        .human => {
            var buf: [512]u8 = undefined;
            const line = std.fmt.bufPrint(&buf, "error: {s}\n  \u{2192} {s}\n", .{ e.message, e.suggestion }) catch "error\n";
            writeErr(line);
        },
    }
    return 1;
}

const JsonError = struct { ok: bool, @"error": CliError };

/// CLI errors shared across handlers — single-sourced so the same condition
/// renders the same code/message/suggestion everywhere (RULE UFS).
pub const ERR_API_URL_UNSET = CliError{ .code = "API_URL_UNSET", .message = "control-plane URL not set", .suggestion = "pass --api <url> or set ZOMBIE_API_URL" };
pub const ERR_UNREACHABLE = CliError{ .code = "CONTROL_PLANE_UNREACHABLE", .message = "could not reach the control plane", .suggestion = "verify --api/ZOMBIE_API_URL and that zombied is up" };

pub fn writeOut(s: []const u8) void {
    std.fs.File.stdout().writeAll(s) catch {};
}

pub fn writeErr(s: []const u8) void {
    std.fs.File.stderr().writeAll(s) catch {};
}

test "audience honours --json regardless of TTY" {
    try std.testing.expectEqual(Audience.json, audience(true));
}

test "structured error serialises the pillars envelope" {
    const alloc = std.testing.allocator;
    const env = JsonError{ .ok = false, .@"error" = .{ .code = "SAMPLE_CODE", .message = "m", .suggestion = "s" } };
    const s = try std.json.Stringify.valueAlloc(alloc, env, .{});
    defer alloc.free(s);
    try std.testing.expect(std.mem.indexOf(u8, s, "\"ok\":false") != null);
    try std.testing.expect(std.mem.indexOf(u8, s, "\"code\":\"SAMPLE_CODE\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, s, "\"suggestion\":\"s\"") != null);
}
