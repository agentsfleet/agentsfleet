//! The SHIPPED incident crew's bundles, asserted as the parser and the gate
//! actually see them (M157 §3, §4).
//!
//! These read `library/incident-*/` from disk rather than a fixture on purpose.
//! Every property here is one a bundle AUTHOR can silently break by editing
//! markdown — an omitted `tools` array, a gate rule that parses but matches
//! nothing, a `repository_access` raised from read to write — and none of them
//! would fail any other test in the tree. Tests run from the repo root (zig
//! build sets cwd), so the paths are relative to it.

const std = @import("std");
const common = @import("common");
const config = @import("config.zig");
const approval_gate = @import("approval_gate.zig");
const integration = @import("../credentials/integration.zig");
const cred_testing = @import("../credentials/testing.zig");
const github = @import("../credentials/integration_github.zig");

const BYTES_PER_KIB = 1024;
const LIBRARY_BASE = "library";
const RESPONDER = "incident-responder";
const REPAIRER = "incident-repairer";
const SKILL_MD = "SKILL.md";
const TRIGGER_MD = "TRIGGER.md";

const HANDLE_GH = "{\"integration\":\"github\",\"installation_id\":\"42\"}";
const TEST_NOW_MS: i64 = 1_700_000_000_000;

/// The event a wake actually carries. The pre-lease gate matches a rule's
/// `tool` against the event TYPE and its `action` against the event ACTOR —
/// not against a tool call — so this is the pair a shipped rule must match.
const WAKE_EVENT_TYPE = "chat";
const WAKE_ACTOR = "steer:user_42";

fn loadBundleFile(alloc: std.mem.Allocator, slug: []const u8, file: []const u8) ![]u8 {
    const path = try std.fs.path.join(alloc, &.{ LIBRARY_BASE, slug, file });
    defer alloc.free(path);
    return std.Io.Dir.cwd().readFileAlloc(common.globalIo(), path, alloc, .limited(64 * BYTES_PER_KIB));
}

fn parseTrigger(alloc: std.mem.Allocator, slug: []const u8) !config.ParsedTrigger {
    const md = try loadBundleFile(alloc, slug, TRIGGER_MD);
    defer alloc.free(md);
    return config.parseTriggerMarkdownWithJson(alloc, md);
}

fn hasTool(cfg: config.FleetConfig, name: []const u8) bool {
    for (cfg.tools) |t| if (std.mem.eql(u8, t, name)) return true;
    return false;
}

fn containsAny(haystack: []const u8, needles: []const []const u8) bool {
    for (needles) |n| if (std.ascii.indexOfIgnoreCase(haystack, n) != null) return true;
    return false;
}

/// A create-installation-access-token response stating that the minted token
/// reaches exactly `declared`. The mint verifies that stated reach against the
/// binding (`integration_github_reach.zig`), so a fake answering anything else
/// fails the mint for a reason the calling test is not about. Caller owns.
fn reachResponse(alloc: std.mem.Allocator, declared: []const []const u8) ![]u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(alloc);
    try out.appendSlice(alloc, "{\"token\":\"ghs_minted\",\"repositories\":[");
    for (declared, 0..) |full_name, i| {
        if (i > 0) try out.append(alloc, ',');
        const entry = try std.fmt.allocPrint(alloc, "{{\"full_name\":\"{s}\"}}", .{full_name});
        defer alloc.free(entry);
        try out.appendSlice(alloc, entry);
    }
    try out.appendSlice(alloc, "]}");
    return out.toOwnedSlice(alloc);
}

/// Collapse every run of whitespace to one space. These assertions are about
/// PROSE, and prose in a markdown file wraps wherever the line ran out — so
/// matching raw bytes makes the test fail on a re-wrap rather than on a
/// meaning change, which is the wrong thing to be sensitive to. Caller owns.
fn flatten(alloc: std.mem.Allocator, md: []const u8) ![]u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(alloc);
    var in_space = false;
    for (md) |c| {
        if (std.ascii.isWhitespace(c)) {
            if (!in_space) try out.append(alloc, ' ');
            in_space = true;
        } else {
            try out.append(alloc, c);
            in_space = false;
        }
    }
    return out.toOwnedSlice(alloc);
}

test "test_repairer_bundle_declares_a_gate" {
    // Dimension 3.2. `approval_gate` falls through to `.auto_approve` when no
    // rule matches, so "declares a gate" is not satisfied by a gates block that
    // merely parses — an autonomous agent holding a write token is what an
    // unmatched rule actually produces.
    const alloc = std.testing.allocator;
    var parsed = try parseTrigger(alloc, REPAIRER);
    defer parsed.deinit(alloc);

    const gates = parsed.config.gates orelse return error.TestUnexpectedResult;
    try std.testing.expect(gates.rules.len > 0);

    // The load-bearing half: the declared rule must MATCH the wake this fleet
    // actually receives, and must ask rather than kill.
    const decision = approval_gate.evaluateGate(gates, WAKE_EVENT_TYPE, WAKE_ACTOR, null);
    try std.testing.expectEqual(approval_gate.GateDecision.requires_approval, decision);

    // The card has to say what it is approving. A blank field renders as
    // nothing, which is how a gate ends up asking a human to approve "something".
    const rule = approval_gate.matchRule(gates, WAKE_EVENT_TYPE, WAKE_ACTOR, null) orelse
        return error.TestUnexpectedResult;
    try std.testing.expect(rule.gate_kind.len > 0);
    try std.testing.expect(rule.blast_radius.len > 0);
}

test "test_bundles_declare_explicit_tools" {
    // Dimension 4.10. `runner_helpers` falls back to the FULL default tool set
    // when `tools` is absent or not an array, so an omitted list does not mean
    // "no tools" — it means every tool NullClaw ships, chosen by nobody.
    const alloc = std.testing.allocator;

    var responder = try parseTrigger(alloc, RESPONDER);
    defer responder.deinit(alloc);
    try std.testing.expect(responder.config.tools.len > 0);
    try std.testing.expect(hasTool(responder.config, "http_request"));
    // Dimension 4.9's dedup cannot run without these two.
    try std.testing.expect(hasTool(responder.config, "memory_store"));
    try std.testing.expect(hasTool(responder.config, "memory_recall"));
    // The investigator reads; it never fetches a tree and never runs git.
    try std.testing.expect(!hasTool(responder.config, "repo_fetch"));
    try std.testing.expect(!hasTool(responder.config, "git"));

    var repairer = try parseTrigger(alloc, REPAIRER);
    defer repairer.deinit(alloc);
    try std.testing.expect(repairer.config.tools.len > 0);
    try std.testing.expect(hasTool(repairer.config, "repo_fetch"));
    try std.testing.expect(hasTool(repairer.config, "git"));
    try std.testing.expect(hasTool(repairer.config, "http_request"));

    // Neither crew member gets a shell. The repair is `git revert` output, and a
    // shell is how that stops being true.
    try std.testing.expect(!hasTool(responder.config, "shell"));
    try std.testing.expect(!hasTool(repairer.config, "shell"));
}

test "test_bundles_declare_degradation" {
    // Dimension 4.8. `runner_progress` observes the context threshold and logs
    // it; NullClaw exposes no mid-loop interrupt, and `continuationActor` has
    // zero callers — so a bundle that promises to resume is promising something
    // the runtime cannot deliver. Both bundles must instead name what they did
    // and did not do, and neither may imply a follow-up run.
    const alloc = std.testing.allocator;

    for ([_][]const u8{ RESPONDER, REPAIRER }) |slug| {
        const raw = try loadBundleFile(alloc, slug, SKILL_MD);
        defer alloc.free(raw);
        const md = try flatten(alloc, raw);
        defer alloc.free(md);

        // Instructs a NAMED degradation, not merely "stop".
        try std.testing.expect(containsAny(md, &.{"named degradation"}));
        // States outright that nothing resumes it.
        try std.testing.expect(containsAny(md, &.{ "no continuation", "nothing continues you" }));
        // And never promises one. The phrase is allowed ONLY inside an
        // instruction forbidding it, which is how both bundles use it.
        if (std.ascii.indexOfIgnoreCase(md, "continuing in the next run")) |_| {
            try std.testing.expect(containsAny(md, &.{"not end with \"continuing in the next run\""}));
        }
    }
}

test "test_investigator_token_is_read_only" {
    // Dimension 3.1. The investigator MUST reach GitHub — it cannot name a
    // suspect commit without reading history — so the boundary is not its host
    // allowlist and not its prompt. It is the MINT: the bundle declares `read`,
    // and a read binding yields a token GitHub itself will refuse a Pull Request
    // from. This drives the SHIPPED binding through the real mint rather than a
    // hand-built one, so editing the bundle to `write` fails here.
    const alloc = std.testing.allocator;

    var responder = try parseTrigger(alloc, RESPONDER);
    defer responder.deinit(alloc);
    const binding = responder.config.repository_binding orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(integration.RepositoryAccess.read, binding.access);
    try std.testing.expect(binding.repositories.len > 0);

    // The fake answers with exactly the reach the SHIPPED bundle declared, so
    // this test keeps failing on the permission level rather than on the mint's
    // reach check — which is a different Dimension with its own tests.
    const reach = try reachResponse(alloc, binding.repositories);
    defer alloc.free(reach);

    var gh = cred_testing.FakeGitHub{ .alloc = alloc, .status = 201, .resp_body = reach };
    defer gh.deinit();
    var h = try cred_testing.parse(alloc, HANDLE_GH);
    defer h.deinit();

    const out = try github.mint(cred_testing.githubCtxBound(
        alloc,
        h.value,
        &gh,
        TEST_NOW_MS,
        .{ .repositories = binding.repositories, .access = binding.access },
    ));
    try std.testing.expect(out == .ok);
    alloc.free(out.ok.token);

    try std.testing.expect(std.mem.indexOf(u8, gh.body, "\"contents\":\"read\"") != null);
    // The absence is the whole Dimension: no pull-requests permission at all,
    // not a pull-requests permission set to read.
    try std.testing.expect(std.mem.indexOf(u8, gh.body, "pull_requests") == null);
}

test "test_crew_holds_no_tenant_key" {
    // Dimension 3.5. A tenant `agt_t` key is how an automation reaches the
    // control plane, and `fleet:write` covers both waking a fleet and rewriting
    // its `gates` block — so a crew member holding one could delete the very
    // approval that guards the repairer. Neither bundle declares one, which is
    // why a HUMAN wakes the repairer in this workstream.
    //
    // The assertion is on the credential NAMES the bundles declare: a tenant key
    // would have to arrive as a workspace secret to be usable at all, and this
    // is where that would show up.
    const alloc = std.testing.allocator;

    for ([_][]const u8{ RESPONDER, REPAIRER }) |slug| {
        var parsed = try parseTrigger(alloc, slug);
        defer parsed.deinit(alloc);
        for (parsed.config.credentials) |name| {
            // Any spelling that would carry a control-plane key for this product.
            try std.testing.expect(std.ascii.indexOfIgnoreCase(name, "agentsfleet") == null);
            try std.testing.expect(std.ascii.indexOfIgnoreCase(name, "tenant") == null);
            try std.testing.expect(std.ascii.indexOfIgnoreCase(name, "agt_t") == null);
        }
    }

    // And neither reaches the control plane's own host, which is the other way a
    // key could be used even if it were named something innocuous.
    var repairer = try parseTrigger(alloc, REPAIRER);
    defer repairer.deinit(alloc);
    const net = repairer.config.network orelse return error.TestUnexpectedResult;
    for (net.allow) |host| {
        try std.testing.expect(std.ascii.indexOfIgnoreCase(host, "agentsfleet.net") == null);
        try std.testing.expect(std.ascii.indexOfIgnoreCase(host, "agentsfleet.dev") == null);
    }
}

test "the repairer is bound for write and the investigator is not" {
    // The two halves of the boundary, asserted together so a copy-paste edit
    // that gives the investigator the repairer's binding cannot pass.
    const alloc = std.testing.allocator;

    var repairer = try parseTrigger(alloc, REPAIRER);
    defer repairer.deinit(alloc);
    const repairer_binding = repairer.config.repository_binding orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(integration.RepositoryAccess.write, repairer_binding.access);

    var responder = try parseTrigger(alloc, RESPONDER);
    defer responder.deinit(alloc);
    const responder_binding = responder.config.repository_binding orelse return error.TestUnexpectedResult;
    try std.testing.expect(responder_binding.access != repairer_binding.access);
}
