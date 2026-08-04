//! repo_fetch_env.zig — the environment a fetch's git processes run under, and
//! the one place the minted credential is written.
//!
//! Split from `repo_fetch_exec.zig` because it answers a different question:
//! that file decides which git steps run, this one decides what those steps can
//! see. It is also the security-critical half, so it is worth reading and
//! testing on its own.
//!
//! Two rules shape all of it.
//!
//! THE CREDENTIAL RIDES THE ENVIRONMENT, NOT ARGV AND NOT DISK. On Linux
//! `/proc/PID/cmdline` is world-readable while `/proc/PID/environ` is
//! owner-only, so a token in argv is readable by any account on the host. And
//! because the caller fetches by URL rather than adding a remote, git writes no
//! credential into `.git/config` either — the tree the sandboxed child inherits
//! carries the remote URL at most and the token never (Invariant 9), which is
//! what lets a test simply grep the fetched tree.
//!
//! NOTHING IS INHERITED. The daemon's own environ carries
//! `AGENTSFLEET_RUNNER_TOKEN` and the control-plane URL, and git has no use for
//! either; the host's `~/.gitconfig` and `/etc/gitconfig` are excluded for the
//! same reason the fetch is bounded — an operator's credential helper, hook, or
//! `insteadOf` rewrite is not part of this contract and must not be able to
//! redirect a fleet's fetch.

const std = @import("std");
const repo_fetch = @import("repo_fetch.zig");

pub const Error = error{
    /// The token is longer than the header buffer admits. Refused rather than
    /// truncated into a header that would fail at the vendor with no local
    /// explanation, and rather than silently fetching unauthenticated.
    CredentialUnusable,
    OutOfMemory,
};

/// Build the COMPLETE environment for one fetch's git processes: the minimum
/// git needs to run, the switches that stop it reading host configuration or
/// prompting, and — only when a token was minted — the URL-scoped authorization
/// header. Allocated from the caller's arena; caller `deinit`s the map.
pub fn build(arena: std.mem.Allocator, remote_url: []const u8, token: []const u8) Error!std.process.Environ.Map {
    var env: std.process.Environ.Map = .init(arena);
    errdefer env.deinit();

    // git execs its transport helpers (`git-remote-https`) by name.
    try env.put(ENV_PATH, GIT_HELPER_PATH);
    // Neither the host's global nor its system config participates.
    try env.put(ENV_GIT_CONFIG_GLOBAL, NULL_DEVICE);
    try env.put(ENV_GIT_CONFIG_SYSTEM, NULL_DEVICE);
    // A credential prompt would block until the deadline killed the step; refuse
    // to authenticate rather than hang, and let the exit status say so.
    try env.put(ENV_GIT_TERMINAL_PROMPT, TERMINAL_PROMPT_OFF);

    if (token.len == 0) return env;
    if (token.len > MAX_TOKEN_LEN) return error.CredentialUnusable;

    // `Basic base64("x-access-token:<token>")` is how a GitHub App installation
    // token authenticates over HTTPS. Scoped to this remote's URL so it is
    // presented to nothing else, even if git were redirected.
    var credential_buf: [MAX_CREDENTIAL_LEN]u8 = undefined;
    var header_buf: [MAX_HEADER_LEN]u8 = undefined;
    var key_buf: [MAX_CONFIG_KEY_LEN]u8 = undefined;
    // Wipe the stack copies on the way out: the map keeps its own duplicate, and
    // leaving a second one in a frame that will be reused is free to avoid.
    defer {
        @memset(&credential_buf, 0);
        @memset(&header_buf, 0);
    }

    const credential = std.fmt.bufPrint(&credential_buf, "{s}:{s}", .{ GITHUB_TOKEN_USERNAME, token }) catch
        return error.CredentialUnusable;
    const encoder = std.base64.standard.Encoder;
    if (encoder.calcSize(credential.len) > header_buf.len - AUTHORIZATION_PREFIX.len) return error.CredentialUnusable;
    @memcpy(header_buf[0..AUTHORIZATION_PREFIX.len], AUTHORIZATION_PREFIX);
    const encoded = encoder.encode(header_buf[AUTHORIZATION_PREFIX.len..], credential);

    const key = std.fmt.bufPrint(&key_buf, "{s}{s}{s}", .{ CONFIG_HTTP_PREFIX, remote_url, CONFIG_EXTRAHEADER_SUFFIX }) catch
        return error.CredentialUnusable;

    try env.put(ENV_GIT_CONFIG_COUNT, ONE_CONFIG_ENTRY);
    try env.put(ENV_GIT_CONFIG_KEY_0, key);
    try env.put(ENV_GIT_CONFIG_VALUE_0, header_buf[0 .. AUTHORIZATION_PREFIX.len + encoded.len]);
    return env;
}

/// git's own `$PATH` — only what it needs to exec its transport helpers.
const GIT_HELPER_PATH = "/usr/bin:/bin:/usr/local/bin:/opt/homebrew/bin";
const NULL_DEVICE = "/dev/null";
const TERMINAL_PROMPT_OFF = "0";
const ONE_CONFIG_ENTRY = "1";

const ENV_PATH = "PATH";
const ENV_GIT_CONFIG_GLOBAL = "GIT_CONFIG_GLOBAL";
const ENV_GIT_CONFIG_SYSTEM = "GIT_CONFIG_SYSTEM";
const ENV_GIT_TERMINAL_PROMPT = "GIT_TERMINAL_PROMPT";
const ENV_GIT_CONFIG_COUNT = "GIT_CONFIG_COUNT";
const ENV_GIT_CONFIG_KEY_0 = "GIT_CONFIG_KEY_0";
const ENV_GIT_CONFIG_VALUE_0 = "GIT_CONFIG_VALUE_0";

const CONFIG_HTTP_PREFIX = "http.";
const CONFIG_EXTRAHEADER_SUFFIX = ".extraheader";
const AUTHORIZATION_PREFIX = "Authorization: Basic ";
/// The username half of a GitHub App installation token's basic credential.
const GITHUB_TOKEN_USERNAME = "x-access-token";

/// Credential ceilings. A GitHub installation token is well under this.
const MAX_TOKEN_LEN: usize = 512;
const MAX_CREDENTIAL_LEN: usize = GITHUB_TOKEN_USERNAME.len + 1 + MAX_TOKEN_LEN;
const BASE64_GROUP_IN: usize = 3;
const BASE64_GROUP_OUT: usize = 4;
const MAX_HEADER_LEN: usize = AUTHORIZATION_PREFIX.len +
    (MAX_CREDENTIAL_LEN + BASE64_GROUP_IN - 1) / BASE64_GROUP_IN * BASE64_GROUP_OUT;
const MAX_CONFIG_KEY_LEN: usize = CONFIG_HTTP_PREFIX.len + repo_fetch.MAX_REMOTE_URL_LEN + CONFIG_EXTRAHEADER_SUFFIX.len;

// ── Tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;
const TEST_URL = "https://github.com/acme/payments.git";

test "the host's own git configuration never participates" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var env = try build(arena.allocator(), TEST_URL, "");
    defer env.deinit();

    // A developer's `~/.gitconfig` credential helper or `insteadOf` rewrite must
    // not be able to redirect or authenticate a fleet's fetch.
    try testing.expectEqualStrings(NULL_DEVICE, env.get(ENV_GIT_CONFIG_GLOBAL).?);
    try testing.expectEqualStrings(NULL_DEVICE, env.get(ENV_GIT_CONFIG_SYSTEM).?);
    // A prompt would block until the deadline killed the step.
    try testing.expectEqualStrings(TERMINAL_PROMPT_OFF, env.get(ENV_GIT_TERMINAL_PROMPT).?);
}

test "an unauthenticated fetch carries no authorization config at all" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var env = try build(arena.allocator(), TEST_URL, "");
    defer env.deinit();

    // Not an empty header — no header. A blank `extraheader` would still be sent.
    try testing.expect(env.get(ENV_GIT_CONFIG_COUNT) == null);
    try testing.expect(env.get(ENV_GIT_CONFIG_KEY_0) == null);
    try testing.expect(env.get(ENV_GIT_CONFIG_VALUE_0) == null);
}

test "a minted token becomes a URL-scoped basic header and nothing else" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const token = "installation-token-under-test";
    var env = try build(arena.allocator(), TEST_URL, token);
    defer env.deinit();

    // Scoped to THIS remote, so a redirect elsewhere is not authenticated.
    try testing.expectEqualStrings(
        CONFIG_HTTP_PREFIX ++ TEST_URL ++ CONFIG_EXTRAHEADER_SUFFIX,
        env.get(ENV_GIT_CONFIG_KEY_0).?,
    );

    const value = env.get(ENV_GIT_CONFIG_VALUE_0).?;
    try testing.expect(std.mem.startsWith(u8, value, AUTHORIZATION_PREFIX));
    // The token is present only in its encoded form — never as plain bytes any
    // scanner of the environment (or of a log line quoting it) would recognize.
    try testing.expect(std.mem.indexOf(u8, value, token) == null);

    var decoded: [MAX_CREDENTIAL_LEN]u8 = undefined;
    const encoded = value[AUTHORIZATION_PREFIX.len..];
    const decoder = std.base64.standard.Decoder;
    const len = try decoder.calcSizeForSlice(encoded);
    try decoder.decode(decoded[0..len], encoded);
    try testing.expectEqualStrings(GITHUB_TOKEN_USERNAME ++ ":" ++ token, decoded[0..len]);
}

test "a token too large to present is refused rather than truncated or dropped" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const oversized = "t" ** (MAX_TOKEN_LEN + 1);
    // Fail closed: fetching unauthenticated would surface as an opaque vendor
    // rejection, and truncating would surface as a wrong credential.
    try testing.expectError(error.CredentialUnusable, build(arena.allocator(), TEST_URL, oversized));
    // The largest admissible token still fits every buffer.
    var env = try build(arena.allocator(), TEST_URL, "t" ** MAX_TOKEN_LEN);
    env.deinit();
}
