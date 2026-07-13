//! Mechanical generator for the Markdown JSX (MDX) error-code page (own repo:
//! ~/Projects/docs). `make gen-error-codes` runs this binary and redirects
//! stdout to that file. Grouping is purely by the code's own category token
//! (`UZ-<CAT>-<NNN>`) in first-seen REGISTRY order; no hand-curated prose
//! survives regeneration. Registry titles and hints fill the public table.
const std = @import("std");
const build_options = @import("build_options");
// Imports the two entry tables directly (not error_registry.zig) — that file's
// aggregator `test {}` block reaches outside errors/ (e.g. ../http/handlers/
// common.zig), which would violate this exe's narrower module root.
const entries = @import("error_entries.zig");
const entries_runtime = @import("error_entries_runtime.zig");
const Entry = entries.Entry;
const REGISTRY = entries.ENTRIES ++ entries_runtime.ENTRIES_RUNTIME;
const common = @import("common");

const PRELUDE_BEFORE_VERSION =
    \\---
    \\title: Error codes
    \\description: API error response fields and stable error codes.
    \\type: reference
    \\audience: user
    \\verified: 2026-07-12
    \\product_version:
;

const PRELUDE_AFTER_VERSION =
    \\executable: false
    \\---
    \\
    \\# Error codes
    \\
    \\## Synopsis
    \\
    \\Every API error uses a JSON Problem Details response. Each response carries a stable error code and documentation URL.
    \\
    \\Use the error code for program logic. Use the request identifier when you contact support.
    \\
    \\## Example with output
    \\
    \\```json
    \\{
    \\  "docs_uri": "https://docs.agentsfleet.net/api-reference/error-codes#UZ-AGT-009",
    \\  "title": "Fleet not found",
    \\  "detail": "No fleet with id '0198a7ba-2c1d-7f08-8a45-3e9b6d2f1c70' in this workspace.",
    \\  "error_code": "UZ-AGT-009",
    \\  "request_id": "0198a7b5-3c8d-7e41-9a2b-6f1d4c7e8a05",
    \\  "user_message": "We couldn't find that Fleet. Check the workspace and fleet identifier."
    \\}
    \\```
    \\
    \\## Options
    \\
    \\The error format has no client option. Every response contains the first five fields below.
    \\
    \\A conflict response also contains `current_state`. Some errors contain `user_message`.
    \\
    \\| Field | Effect | Default | Unit or range |
    \\|---|---|---|---|
    \\| `docs_uri` | Links to this error. | Included | URL |
    \\| `title` | Gives a stable short label. | Included | Text |
    \\| `detail` | Explains this failure. | Included | Text that varies by request |
    \\| `error_code` | Identifies the failure for client logic. | Included | Registered code below |
    \\| `request_id` | Identifies the request for support. | Included | Server-generated value |
    \\| `current_state` | Names the state that blocked a change. | Omitted | Included on HTTP 409 responses |
    \\| `user_message` | Gives text safe to show to a user. | Omitted | Included when the error defines one |
    \\
    \\## Errors
    \\
;

const EPILOGUE =
    \\
    \\## Related pages
    \\
    \\- [API introduction](/api-reference/introduction)
    \\- [API scopes](/api-reference/scopes)
    \\- [Troubleshoot fleets](/fleets/troubleshooting)
    \\
;

fn categoryOf(code: []const u8) []const u8 {
    const rest = code["UZ-".len..];
    const dash = std.mem.indexOfScalar(u8, rest, '-').?;
    return rest[0..dash];
}

// Section titles for the generated page — the raw `UZ-<CAT>-` token is an
// internal registry namespace, not something an external reader recognizes
// ("Wh", "Slk", "Gh" mean nothing outside this codebase). Caught by Greptile
// on the docs PR: exposing the namespace abbreviation directly defeats the
// purpose of a de-mudballed public error-codes page. One entry per category
// token that currently exists in REGISTRY (see gen_error_codes_test.zig for
// the completeness check — a category with no entry here falls back to the
// capitalized-token form, which stays legible but not friendly).
const S_API = "API";
const CATEGORY_COVERAGE_BRANCH_QUOTA = 10_000;

const CategoryCopy = struct {
    token: []const u8,
    label: []const u8,
    prevention: []const u8,
};

const DocFix = struct {
    code: []const u8,
    text: []const u8,
};

const DOC_FIXES = [_]DocFix{
    .{ .code = "UZ-INTERNAL-003", .text = "The request could not finish. Retry it, then report the error code and request identifier if it continues." },
    .{ .code = "UZ-AGT-004", .text = "The fleet could not accept work. Check that the fleet exists and is active, then retry." },
    .{ .code = "UZ-STARTUP-001", .text = "The service cannot start because required settings are missing. An operator must add them before retrying startup." },
    .{ .code = "UZ-STARTUP-002", .text = "The service cannot start because one or more settings are invalid. An operator must correct them before retrying startup." },
    .{ .code = "UZ-STARTUP-003", .text = "The service cannot start because a required data service is unreachable. An operator must restore access before retrying startup." },
    .{ .code = "UZ-STARTUP-004", .text = "The service cannot start because its event service is unreachable. An operator must restore access before retrying startup." },
    .{ .code = "UZ-STARTUP-005", .text = "The service cannot start because stored data is not ready. An operator must finish the data update before retrying startup." },
    .{ .code = "UZ-STARTUP-006", .text = "The host lacked enough memory during startup. Free memory or use a larger host, then retry." },
    .{ .code = "UZ-MEM-003", .text = "Saved memory is unavailable. The fleet uses temporary workspace memory until the service recovers." },
};

const CATEGORY_COPY = [_]CategoryCopy{
    .{ .token = "AGT", .label = "Fleets", .prevention = "Use fleet identifiers from the current workspace." },
    .{ .token = S_API, .label = S_API, .prevention = "Limit concurrent requests and honor retry delays." },
    .{ .token = "APIKEY", .label = "API keys", .prevention = "Use current API key identifiers and replace revoked keys." },
    .{ .token = "APPROVAL", .label = "Approvals", .prevention = "Resolve only current approval requests." },
    .{ .token = "AUTH", .label = "Authentication", .prevention = "Keep sign-in sessions and credentials current." },
    .{ .token = "BUNDLE", .label = "Fleet Bundles", .prevention = "Check Fleet Bundle files and limits before upload." },
    .{ .token = "CONN", .label = "Connectors", .prevention = "Check the provider connection before sending requests." },
    .{ .token = "CRED", .label = "Credentials", .prevention = "Create required workspace secrets before starting a run." },
    .{ .token = "EXEC", .label = "Runs", .prevention = "Check runner settings and access before starting work." },
    .{ .token = "FLEETKEY", .label = "Fleet keys", .prevention = "Use a current Fleet API key for the intended fleet." },
    .{ .token = "GH", .label = "GitHub", .prevention = "Keep the GitHub App installed with required repository access." },
    .{ .token = "GRANT", .label = "Integration grants", .prevention = "Use active integration grants approved for the fleet." },
    .{ .token = "INTERNAL", .label = "Service failures", .prevention = "Clients cannot prevent this failure. Keep retry handling ready." },
    .{ .token = "MEM", .label = "Memory", .prevention = "Use an existing fleet and a valid memory category." },
    .{ .token = "MODELS", .label = "Tenant models", .prevention = "Use a model available to the tenant." },
    .{ .token = "PROVIDER", .label = "Model providers", .prevention = "Configure a supported provider, model, and secret." },
    .{ .token = "REQ", .label = "Request", .prevention = "Validate request fields before sending the request." },
    .{ .token = "CATALOG", .label = "Fleet library catalog", .prevention = "Fetch a bundle before publishing a fleet, and unpublish before deleting one." },
    .{ .token = "RUN", .label = "Runners", .prevention = "Keep runner settings and lease health within configured limits." },
    .{ .token = "SLK", .label = "Slack", .prevention = "Keep Slack app credentials, permissions, and clocks current." },
    .{ .token = "STARTUP", .label = "Startup", .prevention = "Operators should verify required service settings before startup." },
    .{ .token = "TOOL", .label = "Tools", .prevention = "Declare every tool used by the fleet." },
    .{ .token = "UUIDV7", .label = "Identifiers", .prevention = "Use identifiers returned by agentsfleet." },
    .{ .token = "VAULT", .label = "Secrets", .prevention = "Use a valid workspace secret name and value." },
    .{ .token = "WH", .label = "Webhooks", .prevention = "Keep webhook signing secrets matched and service clocks synchronized." },
};

fn categoryCopy(cat: []const u8) CategoryCopy {
    for (CATEGORY_COPY) |copy| {
        if (std.mem.eql(u8, copy.token, cat)) return copy;
    }
    unreachable;
}

comptime {
    @setEvalBranchQuota(CATEGORY_COVERAGE_BRANCH_QUOTA);
    for (REGISTRY) |entry| {
        const cat = categoryOf(entry.code);
        var found = false;
        for (CATEGORY_COPY) |copy| {
            if (std.mem.eql(u8, copy.token, cat)) found = true;
        }
        if (!found) @compileError("missing public copy for error category " ++ cat);
    }
}

fn writeCategoryHeading(w: *std.Io.Writer, cat: []const u8) !void {
    try w.writeAll("### ");
    try w.writeAll(categoryCopy(cat).label);
    try w.writeByte('\n');
}

fn preventionFor(entry: Entry) []const u8 {
    return categoryCopy(categoryOf(entry.code)).prevention;
}

fn publicFix(entry: Entry) []const u8 {
    if (entry.user_message) |message| return message;
    for (DOC_FIXES) |fix| {
        if (std.mem.eql(u8, fix.code, entry.code)) return fix.text;
    }
    return entry.hint;
}

/// Pure render: REGISTRY -> mdx text on `w`. Exposed so a test can prove
/// idempotency without touching stdout or the docs-repo file.
pub fn render(alloc: std.mem.Allocator, w: *std.Io.Writer) !void {
    var order: std.ArrayList([]const u8) = .empty;
    defer order.deinit(alloc);
    var groups: std.ArrayList(std.ArrayList(Entry)) = .empty;
    defer {
        for (groups.items) |*g| g.deinit(alloc);
        groups.deinit(alloc);
    }

    for (REGISTRY) |entry| {
        const cat = categoryOf(entry.code);
        var idx: ?usize = null;
        for (order.items, 0..) |c, i| {
            if (std.mem.eql(u8, c, cat)) {
                idx = i;
                break;
            }
        }
        const group_idx = idx orelse blk: {
            try order.append(alloc, cat);
            try groups.append(alloc, .empty);
            break :blk order.items.len - 1;
        };
        try groups.items[group_idx].append(alloc, entry);
    }

    try w.writeAll(PRELUDE_BEFORE_VERSION);
    try w.writeByte(' ');
    try w.writeAll(build_options.version);
    try w.writeByte('\n');
    try w.writeAll(PRELUDE_AFTER_VERSION);
    for (order.items, 0..) |cat, i| {
        try w.writeByte('\n');
        try writeCategoryHeading(w, cat);
        try w.writeAll("\n| Code | HTTP | Title | Why and fix | Prevent |\n|---|---|---|---|---|\n");
        for (groups.items[i].items) |e| {
            try w.print("| <span id=\"{s}\"></span>`{s}` | {d} | {s} | {s} | {s} |\n", .{
                e.code,
                e.code,
                @intFromEnum(e.http_status),
                e.title,
                publicFix(e),
                preventionFor(e),
            });
        }
    }
    try w.writeAll(EPILOGUE);
}

pub fn main() !void {
    var gpa: std.heap.DebugAllocator(.{}) = .init;
    defer _ = gpa.deinit();
    const alloc = gpa.allocator();
    const io = common.globalIo();

    var stdout_buf: [16384]u8 = undefined;
    var stdout_w = std.Io.File.stdout().writer(io, &stdout_buf);
    const w = &stdout_w.interface;

    try render(alloc, w);
    try w.flush();
}
