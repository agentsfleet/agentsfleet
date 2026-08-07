//! The SHIPPED incident crew's bundles, asserted as the parser and the mint
//! actually see them. One member ships today — the responder — and the crew
//! grows beside the platform half that reads each new member's output.
//!
//! These read `library/incident-*/` from disk rather than a fixture on purpose.
//! Every property here is one a bundle AUTHOR can silently break by editing
//! markdown — an omitted `tools` array, a `repository_access` raised from read
//! to write — and none of them would fail any other test in the tree. Tests run
//! from the repo root (zig build sets cwd), so the paths are relative to it.

const std = @import("std");
const common = @import("common");
const config = @import("config.zig");
const integration = @import("../credentials/integration.zig");
const cred_testing = @import("../credentials/testing.zig");
const github = @import("../credentials/integration_github.zig");

const BYTES_PER_KIB = 1024;
const LIBRARY_BASE = "library";
const RESPONDER = "incident-responder";
const SKILL_MD = "SKILL.md";
const TRIGGER_MD = "TRIGGER.md";

const HANDLE_GH = "{\"integration\":\"github\",\"installation_id\":\"42\"}";
const TEST_NOW_MS: i64 = 1_700_000_000_000;

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

test "test_bundles_declare_explicit_tools" {
    // `runner_helpers` falls back to the FULL default tool set when `tools` is
    // absent or not an array, so an omitted list does not mean "no tools" — it
    // means every tool NullClaw ships, chosen by nobody.
    const alloc = std.testing.allocator;

    var responder = try parseTrigger(alloc, RESPONDER);
    defer responder.deinit(alloc);
    try std.testing.expect(responder.config.tools.len > 0);
    try std.testing.expect(hasTool(responder.config, "http_request"));
    // The escalation dedup cannot run without these two.
    try std.testing.expect(hasTool(responder.config, "memory_store"));
    try std.testing.expect(hasTool(responder.config, "memory_recall"));
    // The investigator reads. It never holds a working tree, never runs git,
    // and never gets a shell — the platform writes, and only approved bytes.
    try std.testing.expect(!hasTool(responder.config, "git"));
    try std.testing.expect(!hasTool(responder.config, "shell"));
    try std.testing.expect(!hasTool(responder.config, "file_write"));
}

test "test_bundles_declare_degradation" {
    // `runner_progress` observes the context threshold and logs it; NullClaw
    // exposes no mid-loop interrupt, and `continuationActor` has zero callers —
    // so a bundle that promises to resume is promising something the runtime
    // cannot deliver. The bundle must instead name what it did and did not do,
    // and may not imply a follow-up run.
    const alloc = std.testing.allocator;

    const raw = try loadBundleFile(alloc, RESPONDER, SKILL_MD);
    defer alloc.free(raw);
    const md = try flatten(alloc, raw);
    defer alloc.free(md);

    // Instructs a NAMED degradation, not merely "stop".
    try std.testing.expect(containsAny(md, &.{"named degradation"}));
    // States outright that nothing resumes it.
    try std.testing.expect(containsAny(md, &.{ "no continuation", "nothing continues you" }));
    // And never promises one. The phrase is allowed ONLY inside an
    // instruction forbidding it, which is how the bundle uses it.
    if (std.ascii.indexOfIgnoreCase(md, "continuing in the next run")) |_| {
        try std.testing.expect(containsAny(md, &.{"not end with \"continuing in the next run\""}));
    }
}

test "test_investigator_token_is_read_only" {
    // The investigator MUST reach GitHub — it cannot name a suspect commit
    // without reading history — so the boundary is not its host allowlist and
    // not its prompt. It is the MINT: the bundle declares `read`, and a read
    // binding yields a token GitHub itself will refuse a Pull Request from.
    // This drives the SHIPPED binding through the real mint rather than a
    // hand-built one, so editing the bundle to `write` fails here.
    const alloc = std.testing.allocator;

    var responder = try parseTrigger(alloc, RESPONDER);
    defer responder.deinit(alloc);
    const binding = responder.config.repository_binding orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(integration.RepositoryAccess.read, binding.access);
    try std.testing.expect(binding.repositories.len > 0);

    // The fake answers with exactly the reach the SHIPPED bundle declared, so
    // this test keeps failing on the permission level rather than on the mint's
    // reach check — which has its own tests.
    const reach = try cred_testing.reachResponse(alloc, binding.repositories);
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
    // The absence is the whole property: no pull-requests permission at all,
    // not a pull-requests permission set to read.
    try std.testing.expect(std.mem.indexOf(u8, gh.body, "pull_requests") == null);
}

test "test_crew_holds_no_tenant_key" {
    // A tenant `agt_t` key is how an automation reaches the control plane, and
    // `fleet:write` covers both waking a fleet and rewriting its config — so a
    // crew member holding one could reshape the very platform that bounds it.
    // No bundle declares one.
    //
    // The assertion is on the credential NAMES the bundle declares: a tenant key
    // would have to arrive as a workspace secret to be usable at all, and this
    // is where that would show up.
    const alloc = std.testing.allocator;

    var responder = try parseTrigger(alloc, RESPONDER);
    defer responder.deinit(alloc);
    for (responder.config.credentials) |name| {
        // Any spelling that would carry a control-plane key for this product.
        try std.testing.expect(std.ascii.indexOfIgnoreCase(name, "agentsfleet") == null);
        try std.testing.expect(std.ascii.indexOfIgnoreCase(name, "tenant") == null);
        try std.testing.expect(std.ascii.indexOfIgnoreCase(name, "agt_t") == null);
    }

    // And it does not reach the control plane's own host, which is the other
    // way a key could be used even if it were named something innocuous.
    const net = responder.config.network orelse return error.TestUnexpectedResult;
    for (net.allow) |host| {
        try std.testing.expect(std.ascii.indexOfIgnoreCase(host, "agentsfleet.net") == null);
        try std.testing.expect(std.ascii.indexOfIgnoreCase(host, "agentsfleet.dev") == null);
    }
}

/// Every `${secrets.NAME.FIELD}` the prose references, as NAME strings. The tool
/// bridge substitutes these at egress; a name the bundle never declared reaches
/// the model as an unresolved literal and the call fails before dispatch.
fn referencedCredentials(alloc: std.mem.Allocator, md: []const u8) ![][]const u8 {
    const OPEN = "${secrets.";
    var out: std.ArrayList([]const u8) = .empty;
    errdefer out.deinit(alloc);
    var i: usize = 0;
    while (std.mem.indexOfPos(u8, md, i, OPEN)) |start| {
        const after = start + OPEN.len;
        const dot = std.mem.indexOfScalarPos(u8, md, after, '.') orelse break;
        const close = std.mem.indexOfScalarPos(u8, md, after, '}') orelse break;
        if (dot < close) {
            const name = md[after..dot];
            var seen = false;
            for (out.items) |existing| if (std.mem.eql(u8, existing, name)) {
                seen = true;
                break;
            };
            if (!seen) try out.append(alloc, name);
        }
        i = close + 1;
    }
    return out.toOwnedSlice(alloc);
}

fn declares(cfg: config.FleetConfig, name: []const u8) bool {
    for (cfg.credentials) |c| if (std.mem.eql(u8, c, name)) return true;
    return false;
}

test "test_data_plane_secrets_stay_placeholders" {
    // Two halves, and the second is the one that bites.
    //
    // Data-plane values reach a run ONLY as `${secrets.NAME.FIELD}` placeholders
    // substituted at the tool bridge, so no bundle may carry a raw value — but a
    // placeholder naming a credential the bundle never DECLARED is just as
    // broken, and silently so: `secret_substitution` fails closed, the call dies
    // before dispatch, and the bundle looks correct to every reader.
    //
    // That is not hypothetical. The shipped investigator asked for
    // `${secrets.github.api_token}` when a mintable credential answers only
    // `.token`, and every GitHub call failed before dispatch until a review
    // caught it. This is the same defect one level up: the NAME rather than the
    // field.
    const alloc = std.testing.allocator;

    var parsed = try parseTrigger(alloc, RESPONDER);
    defer parsed.deinit(alloc);

    const skill = try loadBundleFile(alloc, RESPONDER, SKILL_MD);
    defer alloc.free(skill);
    const names = try referencedCredentials(alloc, skill);
    defer alloc.free(names);

    // The prose reaches for something, or the bundle has no data plane.
    try std.testing.expect(names.len > 0);
    for (names) |name| {
        if (!declares(parsed.config, name)) {
            std.debug.print(
                "\n{s}/SKILL.md references ${{secrets.{s}.*}} but TRIGGER.md declares no `{s}` credential\n",
                .{ RESPONDER, name, name },
            );
            return error.UndeclaredCredentialReferenced;
        }
    }

    // And no raw value rides the markdown. A real token would have to appear
    // as bytes; the placeholder form is the only spelling allowed.
    try std.testing.expect(!containsAny(skill, &.{ "ghp_", "ghs_", "xoxb-", "glsa_" }));
}

test "test_undeclared_host_refused" {
    // The sandbox refuses a host outside the bundle's allowlist —
    // `policy_http_request` pins that for `http_request`, and `network/Plan`
    // enforces it tool-agnostically at the namespace. What neither can check is
    // that the SHIPPED bundle declared the host its own prose depends on.
    const alloc = std.testing.allocator;

    var responder = try parseTrigger(alloc, RESPONDER);
    defer responder.deinit(alloc);
    const net = responder.config.network orelse return error.TestUnexpectedResult;
    try std.testing.expect(net.allow.len > 0);

    var has_github = false;
    for (net.allow) |host| {
        // No wildcard may widen the gate — an exact-match allowlist is the whole
        // mechanism (`policy_http_request_test` pins the runtime half).
        try std.testing.expect(std.mem.indexOfScalar(u8, host, '*') == null);
        if (std.mem.eql(u8, host, "api.github.com")) has_github = true;
    }
    // The investigator keeps its GitHub reach — the write boundary is the MINT,
    // not the host list — so a bundle edit dropping it would break the
    // correlation the diagnosis depends on rather than tightening anything.
    try std.testing.expect(has_github);
}
