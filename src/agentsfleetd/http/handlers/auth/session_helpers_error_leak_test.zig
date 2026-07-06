// Regression guard: failFromStoreError() must never pass a raw Zig error
// tag (@errorName(err)) as the caller-visible `detail` to hx.fail(). These
// are pre-auth CLI-login endpoints — an unmatched session_store.Error used
// to leak its Zig identifier (e.g. "UnexpectedRedisReply") straight into
// the RFC 7807 response body. Same bug class server.zig's auth-middleware
// failure path already fenced; this file fences the session-helpers side.
//
// Source-scanning, not an integration test: forcing every session_store.Error
// variant (including an unmapped one) through the real store would need a
// fault-injectable session_store, which doesn't exist. Scanning proves the
// invariant directly — no hx.fail( call inside failFromStoreError() takes
// @errorName( as an argument — and is cheap to keep green.

const std = @import("std");
const common = @import("common");

const SRC_PATH = "src/agentsfleetd/http/handlers/auth/session_helpers.zig";
const FN_START_MARKER = "pub fn failFromStoreError(";
const HX_FAIL_MARKER = "hx.fail(";

/// String-literal-aware brace/paren matcher: scans from `open_pos + 1`
/// (just past an opening `{`/`(`) for the matching close, treating any
/// `{`/`}`/`(`/`)` inside a `"..."` literal as inert. Without this, a
/// future detail string like `"Use the '}' escape"` would close the depth
/// counter early and silently truncate the scan — mirrors
/// `internal_op_error_sweep_test.zig`'s `matchingCloseParen`.
fn matchingClose(content: []const u8, open_pos: usize, open: u8, close: u8) ?usize {
    var depth: usize = 1;
    var in_string = false;
    var i = open_pos + 1;
    while (i < content.len) : (i += 1) {
        const c = content[i];
        if (in_string) {
            if (c == '\\') {
                i += 1;
            } else if (c == '"') {
                in_string = false;
            }
            continue;
        }
        if (c == '"') {
            in_string = true;
        } else if (c == open) {
            depth += 1;
        } else if (c == close) {
            depth -= 1;
            if (depth == 0) return i;
        }
    }
    return null;
}

/// Returns the `failFromStoreError` function body (from its signature to the
/// matching closing brace), so the scan below is scoped to this function
/// only — other functions in the file are free to do whatever they need.
fn extractFunctionBody(src: []const u8) ![]const u8 {
    const start = std.mem.indexOf(u8, src, FN_START_MARKER) orelse return error.FunctionNotFound;
    const brace_open = std.mem.indexOfScalarPos(u8, src, start, '{') orelse return error.FunctionNotFound;
    const brace_close = matchingClose(src, brace_open, '{', '}') orelse return error.FunctionNotFound;
    return src[start .. brace_close + 1];
}

/// Fails if any `hx.fail(` call within `body` passes `@errorName(` as an
/// argument (scoped to the call's own parens, not just "appears somewhere
/// on the line" — a future multi-line call shouldn't slip past this).
fn scanForRawErrorNameInFail(body: []const u8) !void {
    var idx: usize = 0;
    while (std.mem.indexOfPos(u8, body, idx, HX_FAIL_MARKER)) |pos| {
        const open_paren = pos + HX_FAIL_MARKER.len - 1;
        const close_paren = matchingClose(body, open_paren, '(', ')') orelse body.len;
        idx = close_paren + 1;

        const args = body[open_paren + 1 .. close_paren];
        if (std.mem.indexOf(u8, args, "@errorName(") != null) {
            std.debug.print("hx.fail() call passes a raw @errorName(...) tag as detail: \"{s}\"\n", .{args});
            return error.RawErrorNameLeaksToWire;
        }
    }
}

test "failFromStoreError() never passes a raw @errorName tag to hx.fail()" {
    const alloc = std.testing.allocator;
    const src = try std.Io.Dir.cwd().readFileAlloc(common.globalIo(), SRC_PATH, alloc, .limited(64 * 1024));
    defer alloc.free(src);

    const body = try extractFunctionBody(src);
    try scanForRawErrorNameInFail(body);
}

test "guard: a fixture hx.fail() call with a raw @errorName tag is caught" {
    const fixture =
        \\pub fn failFromStoreError(hx: hx_mod.Hx, err: anyerror, session_id: ?[]const u8) void {
        \\    hx.fail(code, @errorName(err));
        \\}
    ;
    try std.testing.expectError(error.RawErrorNameLeaksToWire, scanForRawErrorNameInFail(fixture));
}

test "guard: a fixture hx.fail() call with a stable literal detail passes" {
    const fixture =
        \\pub fn failFromStoreError(hx: hx_mod.Hx, err: anyerror, session_id: ?[]const u8) void {
        \\    hx.fail(code, "Failed to process the login session");
        \\}
    ;
    try scanForRawErrorNameInFail(fixture);
}

// Greptile caught: a naive brace/paren counter (no string awareness) closes
// early on an unbalanced `}` or `)` inside a detail literal, truncating the
// scan before the real hx.fail(@errorName(err)) call — a false negative.
// These pin that extractFunctionBody/scanForRawErrorNameInFail see THROUGH
// a brace/paren inside a string, not stop at it.
test "guard: an unbalanced brace inside a detail string doesn't truncate extractFunctionBody" {
    const fixture =
        \\pub fn failFromStoreError(hx: hx_mod.Hx, err: anyerror, session_id: ?[]const u8) void {
        \\    log.warn("evt", .{ .detail = "Use the '}' escape" });
        \\    hx.fail(code, @errorName(err));
        \\}
        \\
        \\pub fn unrelated() void {}
    ;
    const body = try extractFunctionBody(fixture);
    try std.testing.expect(std.mem.indexOf(u8, body, "hx.fail(code, @errorName(err))") != null);
    try std.testing.expectError(error.RawErrorNameLeaksToWire, scanForRawErrorNameInFail(body));
}

test "guard: an unbalanced paren inside a detail string doesn't truncate scanForRawErrorNameInFail" {
    const fixture =
        \\pub fn failFromStoreError(hx: hx_mod.Hx, err: anyerror, session_id: ?[]const u8) void {
        \\    hx.fail(errCodeWithParen("oops)"), @errorName(err));
        \\}
    ;
    try std.testing.expectError(error.RawErrorNameLeaksToWire, scanForRawErrorNameInFail(fixture));
}
