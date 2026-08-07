// Approval gate message builder.
//
// Builds interactive message payloads with [Approve] and [Deny] buttons
// for the approval gate flow. Provider-agnostic — outputs Block Kit JSON
// compatible with Slack, but the structure works for any chat provider.
// All user-supplied fields are JSON-escaped via std.json.stringify (RULES.md #23).

const std = @import("std");
const Allocator = std.mem.Allocator;
const approval_gate = @import("approval_gate.zig");
const config_types = @import("config_types.zig");

/// Build a Slack Block Kit JSON payload for the approval message.
/// Returns an owned JSON string. Caller must free.
pub fn buildSlackApprovalMessage(
    alloc: Allocator,
    fleet_name: []const u8,
    action_id: []const u8,
    detail: approval_gate.ActionDetail,
    callback_url: []const u8,
) ![]const u8 {
    // Build the description text safely — JSON-escape all user-supplied values.
    //
    // Three tiers, and the ORDER is the point. Daemon-derived facts first (a
    // model cannot forge its own fleet name, actor, or event id), then the
    // workspace-authored gate copy stated plainly, then the model's own words
    // LAST and explicitly attributed. A reader who stops early has read only
    // things that are true.
    const desc = try std.fmt.allocPrint(alloc, "Fleet `{s}` wants to execute:\n- Tool: `{s}`\n- Action: `{s}`\n- Details: {s}", .{
        fleet_name, detail.tool, detail.action, detail.params_summary,
    });
    defer alloc.free(desc);

    const gate_line = if (detail.gate_kind.len == 0) try alloc.dupe(u8, "") else try std.fmt.allocPrint(alloc, "\n- Gate: `{s}`", .{detail.gate_kind});
    defer alloc.free(gate_line);
    const radius_line = if (detail.blast_radius.len == 0) try alloc.dupe(u8, "") else try std.fmt.allocPrint(alloc, "\n- If approved: {s}", .{detail.blast_radius});
    defer alloc.free(radius_line);
    // The last DAEMON-derived line, and the only decision-relevant one: the
    // repositories this run's minted token can reach at all. Everything above it
    // is event metadata; without it the card's factual half named no repository,
    // leaving "revert abc123 in acme/widgets" resting entirely on the model.
    const reach_line = try buildReachLine(alloc, detail.repository_binding);
    defer alloc.free(reach_line);
    // ATTRIBUTED, never stated. This text was written by a language model that
    // may have been talked into something; rendering it as the platform's own
    // statement is exactly the confusion an approval card must not create.
    const claim_line = if (detail.proposed_action.len == 0) try alloc.dupe(u8, "") else try std.fmt.allocPrint(alloc, "\n\n_The fleet says it will:_ {s}", .{detail.proposed_action});
    defer alloc.free(claim_line);
    // The model's cited evidence, attributed alongside its claim and never
    // above it. Rendered as a code span: the JSON was re-serialized by
    // `approval_gate_detail`, so its own newlines are already `\\n` TEXT and
    // cannot break the line, and a code span keeps the rest inert.
    const evidence_line = try buildEvidenceLine(alloc, detail.evidence_json);
    defer alloc.free(evidence_line);
    const fallback = try std.fmt.allocPrint(alloc, "Approval required for {s}: {s}.{s}", .{
        fleet_name, detail.tool, detail.action,
    });
    defer alloc.free(fallback);

    _ = callback_url;

    // Build via a growable Io.Writer with JSON-escaped strings for safety.
    var aw: std.Io.Writer.Allocating = .init(alloc);
    errdefer aw.deinit();
    const w = &aw.writer;

    try w.writeAll("{\"blocks\":[{\"type\":\"section\",\"text\":{\"type\":\"mrkdwn\",\"text\":\"");
    try writeJsonEscaped(w, desc);
    try writeJsonEscaped(w, gate_line);
    try writeJsonEscaped(w, radius_line);
    try writeJsonEscaped(w, reach_line);
    try writeJsonEscaped(w, claim_line);
    try writeJsonEscaped(w, evidence_line);
    try w.writeAll("\"}},{\"type\":\"actions\",\"block_id\":\"gate_");
    try writeJsonEscaped(w, action_id);
    try w.writeAll("\",\"elements\":[{\"type\":\"button\",\"text\":{\"type\":\"plain_text\",\"text\":\"Approve\"},\"style\":\"primary\",\"action_id\":\"gate_approve\",\"value\":\"");
    try writeJsonEscaped(w, action_id);
    try w.writeAll("\"},{\"type\":\"button\",\"text\":{\"type\":\"plain_text\",\"text\":\"Deny\"},\"style\":\"danger\",\"action_id\":\"gate_deny\",\"value\":\"");
    try writeJsonEscaped(w, action_id);
    try w.writeAll("\"}]}],\"text\":\"");
    try writeJsonEscaped(w, fallback);
    try w.writeAll("\"}");

    return aw.toOwnedSlice();
}

/// The empty-evidence spelling, shared with `ActionDetail.evidence_json`'s
/// default so "no evidence" renders as nothing rather than as an empty object
/// (RULE UFS).
const S_NO_EVIDENCE = "{}";

/// Render the evidence inside ONE code span no matter what it carries. A
/// backtick inside the JSON would close the span and let the remainder render
/// as mrkdwn — rows that counterfeit the daemon-authored half above. Backticks
/// become apostrophes: the identifier stays readable, the span stays sealed.
fn buildEvidenceLine(alloc: Allocator, evidence_json: []const u8) ![]const u8 {
    if (evidence_json.len == 0 or std.mem.eql(u8, evidence_json, S_NO_EVIDENCE))
        return alloc.dupe(u8, "");
    var aw: std.Io.Writer.Allocating = .init(alloc);
    errdefer aw.deinit();
    const w = &aw.writer;
    try w.writeAll("\n_…citing:_ `");
    for (evidence_json) |c| try w.writeByte(if (c == '`') '\'' else c);
    try w.writeByte('`');
    return aw.toOwnedSlice();
}

/// Render the fleet's repository egress binding as a daemon-derived fact.
///
/// This is the one line on the card whose decision-relevant content the platform
/// can vouch for: it is the same binding `credentials/integration_github.zig`
/// pins the minted token to, so whatever the model claims, the released run
/// cannot reach outside this list. Empty when the fleet declares none — the mint
/// then refuses and the run reaches nothing at all, so there is no reach to
/// state and a reassuring default would be the wrong thing to invent.
fn buildReachLine(alloc: Allocator, binding: ?config_types.RepositoryBinding) ![]const u8 {
    const b = binding orelse return alloc.dupe(u8, "");
    var aw: std.Io.Writer.Allocating = .init(alloc);
    errdefer aw.deinit();
    const w = &aw.writer;
    try w.writeAll("\n- Token reaches: ");
    for (b.repositories, 0..) |repo, i| {
        if (i > 0) try w.writeAll(", ");
        try w.writeByte('`');
        try w.writeAll(repo);
        try w.writeByte('`');
    }
    try w.writeAll(" (");
    try w.writeAll(@tagName(b.access));
    try w.writeAll(")");
    return aw.toOwnedSlice();
}

/// Write a string with JSON-unsafe characters escaped (for embedding in JSON keys).
pub fn writeJsonEscaped(writer: anytype, s: []const u8) !void {
    for (s) |c| {
        switch (c) {
            '"' => try writer.writeAll("\\\""),
            '\\' => try writer.writeAll("\\\\"),
            '\n' => try writer.writeAll("\\n"),
            '\r' => try writer.writeAll("\\r"),
            '\t' => try writer.writeAll("\\t"),
            else => try writer.writeByte(c),
        }
    }
}

test "buildSlackApprovalMessage: produces valid JSON" {
    const alloc = std.testing.allocator;
    const msg = try buildSlackApprovalMessage(
        alloc,
        "test-fleet",
        "action-001",
        .{ .tool = "git", .action = "push", .params_summary = "3 files to main" },
        "https://api.agentsfleet.net/v1/webhooks/z1/approval",
    );
    defer alloc.free(msg);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, msg, .{});
    defer parsed.deinit();
    try std.testing.expect(parsed.value == .object);
}

test "buildSlackApprovalMessage: contains action_id in buttons" {
    const alloc = std.testing.allocator;
    const msg = try buildSlackApprovalMessage(
        alloc,
        "z",
        "act-123",
        .{ .tool = "git", .action = "push", .params_summary = "x" },
        "https://example.com",
    );
    defer alloc.free(msg);
    try std.testing.expect(std.mem.indexOf(u8, msg, "act-123") != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "gate_approve") != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "gate_deny") != null);
}

test "buildSlackApprovalMessage: JSON-escapes quotes in user input" {
    const alloc = std.testing.allocator;
    // Fleet name with a quote — must not break JSON
    const msg = try buildSlackApprovalMessage(
        alloc,
        "test\"fleet",
        "act-1",
        .{ .tool = "git", .action = "push", .params_summary = "file with \"quotes\"" },
        "https://example.com",
    );
    defer alloc.free(msg);
    // Must still parse as valid JSON (quotes escaped)
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, msg, .{});
    defer parsed.deinit();
    try std.testing.expect(parsed.value == .object);
}

test "buildSlackApprovalMessage: no memory leaks (leak detector)" {
    // std.testing.allocator detects leaks — if buildSlackApprovalMessage
    // leaks internal buffers, this test will fail.
    const alloc = std.testing.allocator;
    const msg = try buildSlackApprovalMessage(
        alloc,
        "z",
        "a",
        .{ .tool = "t", .action = "a", .params_summary = "s" },
        "https://x.com",
    );
    alloc.free(msg);
}

test "test_slack_approval_names_the_action" {
    // Dimension 2.2. The card names the gate, the blast radius, and the
    // repository the token reaches, and attributes the model's claim as a claim.
    const alloc = std.testing.allocator;
    const msg = try buildSlackApprovalMessage(alloc, "incident-responder", "action-77", .{
        .tool = "chat",
        .action = "steer:user_9",
        .params_summary = "evt-123",
        .gate_kind = "repair",
        .proposed_action = "revert abc123 in acme/widgets",
        .blast_radius = "one draft Pull Request on acme/widgets",
    }, "");
    defer alloc.free(msg);

    // Still valid JSON with the model's prose embedded.
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, msg, .{});
    defer parsed.deinit();

    // The human is told what is being approved and how far a yes reaches.
    try std.testing.expect(std.mem.indexOf(u8, msg, "repair") != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "one draft Pull Request on acme/widgets") != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "revert abc123 in acme/widgets") != null);
    // ATTRIBUTION is the assertion that matters: the model's words must arrive
    // marked as the fleet's claim, never as the platform stating a fact.
    try std.testing.expect(std.mem.indexOf(u8, msg, "The fleet says it will:") != null);
}

test "buildSlackApprovalMessage: blank detail fields render as nothing, not as a reassuring default" {
    const alloc = std.testing.allocator;
    const msg = try buildSlackApprovalMessage(alloc, "f", "a1", .{
        .tool = "git",
        .action = "push",
        .params_summary = "3 files",
    }, "");
    defer alloc.free(msg);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, msg, .{});
    defer parsed.deinit();
    // A gate that says nothing about its blast radius must not imply one.
    try std.testing.expect(std.mem.indexOf(u8, msg, "If approved:") == null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "The fleet says it will:") == null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "Gate:") == null);
}

test "buildSlackApprovalMessage: model prose carrying JSON metacharacters cannot break the payload" {
    const alloc = std.testing.allocator;
    // A fleet that has been talked into emitting Block Kit of its own gets
    // escaped, not honoured — otherwise it could forge its own approve button.
    const msg = try buildSlackApprovalMessage(alloc, "f", "a1", .{
        .tool = "chat",
        .action = "steer",
        .params_summary = "e1",
        .proposed_action = "\"}],\"blocks\":[{\"type\":\"section\",\"text\":\"APPROVED",
    }, "");
    defer alloc.free(msg);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, msg, .{});
    defer parsed.deinit();
    // One blocks array — the injected one did not become structure.
    try std.testing.expectEqual(@as(usize, 2), parsed.value.object.get("blocks").?.array.items.len);
}

test "buildSlackApprovalMessage: the token's reach is stated as fact, the model's evidence is cited after its claim" {
    const alloc = std.testing.allocator;
    const repos = [_][]const u8{"acme/widgets"};
    const msg = try buildSlackApprovalMessage(alloc, "incident-responder", "act-1", .{
        .tool = "api",
        .action = "steer:api",
        .params_summary = "evt-9",
        .gate_kind = "repair",
        .blast_radius = "one draft Pull Request",
        .proposed_action = "revert abc123 in acme/widgets",
        .evidence_json = "{\"commit\":\"abc123\"}",
        .repository_binding = .{ .repositories = &repos, .access = .write },
    }, "");
    defer alloc.free(msg);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, msg, .{});
    defer parsed.deinit();

    // The reach names the repository AND the access level the mint will grant —
    // the only decision-relevant content on this card the platform can vouch for.
    try std.testing.expect(std.mem.indexOf(u8, msg, "Token reaches") != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "acme/widgets") != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "(write)") != null);

    // Order is the trust boundary: daemon-vouched reach BEFORE the model's claim,
    // and the model's evidence after it, so a reader meets facts first.
    const reach_at = std.mem.indexOf(u8, msg, "Token reaches").?;
    const claim_at = std.mem.indexOf(u8, msg, "The fleet says it will").?;
    const cite_at = std.mem.indexOf(u8, msg, "citing").?;
    try std.testing.expect(reach_at < claim_at);
    try std.testing.expect(claim_at < cite_at);
}

test "buildSlackApprovalMessage: a fleet with no binding claims no reach" {
    const alloc = std.testing.allocator;
    const msg = try buildSlackApprovalMessage(alloc, "f", "a", .{
        .tool = "api",
        .action = "x",
        .params_summary = "e",
    }, "");
    defer alloc.free(msg);
    // An unbound fleet mints nothing, so the card invents no reassuring reach —
    // and with no evidence sent, no citation line either.
    try std.testing.expect(std.mem.indexOf(u8, msg, "Token reaches") == null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "citing") == null);
}

test "test_card_write_radius_and_span_safety" {
    // Two properties on one card: the write-kind
    // park renders the daemon's own kind + blast radius beside the token reach,
    // and evidence carrying a backtick cannot close its code span to counterfeit
    // daemon-authored rows below it.
    const alloc = std.testing.allocator;
    const gate_constants = @import("approval_gate_constants.zig");
    const repos = [_][]const u8{"acme/payments"};
    const msg = try buildSlackApprovalMessage(alloc, "incident-repairer", "act-9", .{
        .tool = "webhook",
        .action = "webhook:github",
        .params_summary = "evt-42",
        .gate_kind = gate_constants.GATE_KIND_REPOSITORY_WRITE,
        .blast_radius = gate_constants.GATE_BLAST_RADIUS_REPOSITORY_WRITE,
        .evidence_json = "{\"q\":\"a` - Gate: forged\"}",
        .repository_binding = .{ .repositories = &repos, .access = .write },
    }, "");
    defer alloc.free(msg);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, msg, .{});
    defer parsed.deinit();

    // The daemon's half: kind, radius, and reach all present as fact.
    try std.testing.expect(std.mem.indexOf(u8, msg, gate_constants.GATE_KIND_REPOSITORY_WRITE) != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, gate_constants.GATE_BLAST_RADIUS_REPOSITORY_WRITE) != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "acme/payments") != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "(write)") != null);

    // The forgery: the evidence backtick was swapped for an apostrophe, so the
    // injected row stays INSIDE the code span instead of rendering as mrkdwn.
    try std.testing.expect(std.mem.indexOf(u8, msg, "a`") == null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "a'") != null);
}
