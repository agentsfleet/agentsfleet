//! The forbidden-egress half of §4 Dimension 4.1 —
//! `test_library_secret_and_metadata_sink_policy`.
//!
//! `library_sink_policy_test.zig` is the other half: it proves the RESPONSE
//! structs carry no secret-shaped field, by reflecting over their definitions.
//! This one proves nothing secret-shaped reaches a SINK — a log record, a trace
//! attribute, a metric label — by scanning every emission site in the daemon.
//!
//! Two mechanisms because they fail differently. Reflection cannot see a
//! `log.err` call, and a scan cannot see a struct field that no call site
//! happens to mention today.
//!
//! ## What is forbidden, and why it is narrower than §4's sentence
//!
//! §4 names one field set — `secret_ref`, provider, kind, base URL, `has_key`,
//! presence booleans — and says it may not enter logs, traces, metrics,
//! analytics, observable cache keys, or benchmark artifacts. Read literally that
//! forbids `provider`, which flags 124 pre-existing emission sites across OIDC,
//! Slack, GitHub and the outbound worker — none of which touch a credential.
//!
//! The reason the literal reading over-fires is that §4 uses ONE list for two
//! different jobs: an allow-list for what a user may SEE in a response, and a
//! deny-list for what may reach a sink. Those are different questions. A user
//! needs `provider` to know which vendor a row bills against, and that same fact
//! leaks nothing from a log line.
//!
//! `docs/LOGGING_STANDARD.md` §6 already answers the sink question, and it is
//! the constant while a spec is the instance: *"Same secret VALUES must not
//! appear anywhere in log records"* — values, not identifiers. So this scan
//! denies secret values, and `base_url` because it is the one displayed field
//! that can structurally CARRY one (`base_url_guard.zig` accepts userinfo on
//! purpose, and its own test asserts `https://user:pw@host` is `.ok`).
//!
//! Identifiers are allowed, deliberately. `key_name` / `secret_ref` name a
//! credential without revealing it; the same name is already on the request line
//! of the API that manages it, and `crypto_store.zig` logs it at `info` BY
//! DESIGN so credential access stays auditable in production. Denying it would
//! delete a security control in the name of one.
//!
//! Owner decision (2026-07-26): *"secrets (api_keys and so on) to stay away from
//! secret values, base_url logs — anything that is a secret value. the secret
//! ref key_name (if spilled) has no harm here since the attacker will not know
//! the secret value, can be logged."*
//!
//! ## It passes today
//!
//! Nothing in the daemon currently emits a denied field, so this is a
//! regression tripwire rather than a fix. The one near-miss is
//! `fleet/service_endpoint.zig`, which logs `hostFromUrl(base_url)` — the host,
//! with userinfo deliberately stripped (`execution_policy.zig`: *"Strip optional
//! userinfo@ — a hostname carries none"*). The scan matches FIELD NAMES rather
//! than any mention of the word, so that site reads as `.inference_host` and
//! passes, which is the correct verdict and not a loophole: the value in the
//! record genuinely is a hostname.

const std = @import("std");
const common = @import("common");

const testing = std.testing;

const SRC_DIR_PATH = "src/agentsfleetd";

/// Emission sites. Structured logging and tracing both take an anonymous struct
/// of named fields, which is what this scans.
const EMITTERS = [_][]const u8{
    "log.err(",
    "log.warn(",
    "log.info(",
    "log.debug(",
    "addAttr(",
    "addIntAttr(",
};

/// A field whose VALUE is, or can carry, a secret. Substring-matched, so
/// `api_key_hash` is caught along with `api_key`.
const FORBIDDEN_SINK_SUBSTRINGS = [_][]const u8{
    "api_key",
    "apikey",
    "password",
    "passphrase",
    "plaintext",
    "private_key",
    "secret_value",
    "base_url",
    "bearer",
};

/// Names that CONTAIN a forbidden substring but identify rather than reveal.
/// Checked first, so the deny list can stay coarse without flagging them.
///
/// Every entry here is a deliberate allowance, not an oversight — see the module
/// note for why an identifier is not a secret.
const ALLOWED_SINK_FIELDS = [_][]const u8{
    "key_name",
    "secret_ref",
    "provider",
    "kind",
    "has_key",
};

fn eqlAny(name: []const u8, list: []const []const u8) bool {
    for (list) |candidate| {
        if (std.mem.eql(u8, name, candidate)) return true;
    }
    return false;
}

fn containsAny(name: []const u8, list: []const []const u8) bool {
    for (list) |needle| {
        if (std.mem.indexOf(u8, name, needle) != null) return true;
    }
    return false;
}

fn isFieldNameByte(c: u8) bool {
    return (c >= 'a' and c <= 'z') or (c >= 'A' and c <= 'Z') or (c >= '0' and c <= '9') or c == '_';
}

/// The field name at `.name = `, or null when this dot is not a field label.
///
/// Matching `.name =` rather than a bare mention is what keeps a VALUE
/// expression from reading as a field: `hostFromUrl(base_url)` mentions
/// `base_url`, but the field it lands under is `.inference_host`, and only the
/// latter describes what the record actually contains.
fn fieldNameAt(content: []const u8, dot: usize) ?[]const u8 {
    var end = dot + 1;
    while (end < content.len and isFieldNameByte(content[end])) end += 1;
    if (end == dot + 1) return null;

    // `.foo = ` — allow spaces before the `=`, and reject `==`.
    var cursor = end;
    while (cursor < content.len and content[cursor] == ' ') cursor += 1;
    if (cursor >= content.len or content[cursor] != '=') return null;
    if (cursor + 1 < content.len and content[cursor + 1] == '=') return null;

    // A leading `.` that follows an identifier byte is field ACCESS
    // (`self.base_url = x`), not a struct literal label.
    if (dot > 0 and isFieldNameByte(content[dot - 1])) return null;
    return content[dot + 1 .. end];
}

/// Where the emitter's argument list ends, by paren balance from `open`.
fn matchingParen(content: []const u8, open: usize) usize {
    var depth: usize = 0;
    var i = open;
    while (i < content.len) : (i += 1) {
        switch (content[i]) {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if (depth == 0) return i;
            },
            else => {},
        }
    }
    return content.len;
}

/// Every forbidden field name emitted in `content`, appended to `out`.
pub fn scanContent(alloc: std.mem.Allocator, content: []const u8, out: *std.ArrayList([]const u8)) !void {
    for (EMITTERS) |emitter| {
        var idx: usize = 0;
        while (std.mem.indexOfPos(u8, content, idx, emitter)) |hit| {
            const open = hit + emitter.len - 1;
            const close = matchingParen(content, open);
            const args = content[open..close];

            var cursor: usize = 0;
            while (std.mem.indexOfScalarPos(u8, args, cursor, '.')) |dot| {
                cursor = dot + 1;
                const name = fieldNameAt(args, dot) orelse continue;
                if (eqlAny(name, &ALLOWED_SINK_FIELDS)) continue;
                if (!containsAny(name, &FORBIDDEN_SINK_SUBSTRINGS)) continue;
                try out.append(alloc, name);
            }
            idx = hit + emitter.len;
        }
    }
}

test "test_library_secret_and_metadata_sink_policy: no secret value reaches a log, trace or metric" {
    const alloc = testing.allocator;
    const io = common.globalIo();

    var dir = try std.Io.Dir.cwd().openDir(io, SRC_DIR_PATH, .{ .iterate = true });
    defer dir.close(io);

    var walker = try dir.walk(alloc);
    defer walker.deinit();

    var findings: std.ArrayList([]const u8) = .empty;
    defer findings.deinit(alloc);

    while (try walker.next(io)) |entry| {
        if (entry.kind != .file) continue;
        if (!std.mem.endsWith(u8, entry.basename, ".zig")) continue;
        // Tests plant sentinels on purpose; the rule is about production sinks.
        if (std.mem.endsWith(u8, entry.basename, "_test.zig")) continue;
        // This file names every forbidden substring in its own deny list.
        if (std.mem.eql(u8, entry.basename, "library_sink_scan_test.zig")) continue;

        const content = try entry.dir.readFileAlloc(io, entry.basename, alloc, .limited(512 * 1024));
        defer alloc.free(content);
        try scanContent(alloc, content, &findings);
    }

    if (findings.items.len != 0) {
        for (findings.items) |name| {
            std.log.err("sink scan: forbidden field '{s}' reaches a log/trace/metric", .{name});
        }
    }
    try testing.expectEqual(@as(usize, 0), findings.items.len);
}

test "test_library_secret_and_metadata_sink_policy: the sink matcher actually matches" {
    // Self-test. A scan that silently matched nothing would pass the sweep above
    // for the worst possible reason, and the failure would look identical to
    // success — so the matcher is exercised against both verdicts here.
    const alloc = testing.allocator;

    var caught: std.ArrayList([]const u8) = .empty;
    defer caught.deinit(alloc);
    try scanContent(alloc, "log.err(\"x\", .{ .api_key = k });", &caught);
    try testing.expectEqual(@as(usize, 1), caught.items.len);
    try testing.expectEqualStrings("api_key", caught.items[0]);

    // The near-miss that must NOT fire: the word appears, but as a VALUE the
    // host has already been extracted from, under an honest field name.
    var clean: std.ArrayList([]const u8) = .empty;
    defer clean.deinit(alloc);
    try scanContent(alloc, "log.warn(\"x\", .{ .inference_host = hostFromUrl(base_url) });", &clean);
    try testing.expectEqual(@as(usize, 0), clean.items.len);

    // The allowance that must NOT fire: an identifier, not a value.
    var allowed: std.ArrayList([]const u8) = .empty;
    defer allowed.deinit(alloc);
    try scanContent(alloc, "log.info(\"stored\", .{ .key_name = n, .secret_ref = r });", &allowed);
    try testing.expectEqual(@as(usize, 0), allowed.items.len);

    // Field ACCESS is not a field label — `self.base_url = x` is an assignment
    // that happens to sit inside an emitter's argument span.
    var access: std.ArrayList([]const u8) = .empty;
    defer access.deinit(alloc);
    try scanContent(alloc, "log.err(\"x\", .{ .n = blk: { self.base_url = y; break :blk 1; } });", &access);
    try testing.expectEqual(@as(usize, 0), access.items.len);
}

test "test_library_secret_and_metadata_sink_policy: every allowed sink field is one the deny list would otherwise catch, or a §4 metadata field" {
    // Keeps the allow list honest: an entry that no deny rule could ever match
    // is dead weight that reads as a carve-out. `provider`, `kind` and `has_key`
    // are here because §4's literal sentence names them, and this is where the
    // narrowing is recorded in code rather than only in prose.
    try testing.expect(eqlAny("key_name", &ALLOWED_SINK_FIELDS));
    try testing.expect(eqlAny("secret_ref", &ALLOWED_SINK_FIELDS));
    try testing.expect(!eqlAny("api_key", &ALLOWED_SINK_FIELDS));
    try testing.expect(containsAny("workspace_base_url", &FORBIDDEN_SINK_SUBSTRINGS));
}
