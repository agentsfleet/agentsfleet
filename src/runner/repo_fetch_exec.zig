//! repo_fetch_exec.zig — the fetch itself: three git steps into a daemon-owned
//! directory, under one shared deadline and one byte ceiling.
//!
//! `repo_fetch.decide` answers whether an ask is allowed. This answers whether
//! it worked. The pair is deliberate: the refusal surface is pure so "refused
//! before any network call" is a unit test, and everything that touches a
//! process, a socket, or a disk lives here behind it.
//!
//! The steps are git's, because the repair rung is a revert and only git
//! computes one correctly. Reconstructing a revert from the vendor's REST API
//! means writing each changed file's bytes back as they were at the parent,
//! which is a revert only if nothing else touched those files since — and after
//! an incident the base has usually moved. git's three-way merge is right, and
//! fails cleanly when it cannot be.
//!
//!   1. `git init`     — an empty object store in the claimed directory.
//!   2. `git fetch`    — the suspect commit, its parent, and the target head, at
//!                       depth 2 over two tips, into two named refs.
//!   3. `git checkout` — a detached working tree at the head, so the child's
//!                       `git revert` has something to apply onto.
//!
//! Fetching by URL rather than adding a remote is load-bearing, not a
//! shorthand: `git fetch <url> <refspec>` writes no remote, so no credential can
//! reach `.git/config` under the target. Where the credential DOES go, and why
//! nothing of the host's git configuration participates, is `repo_fetch_env.zig`.

const std = @import("std");
const logging = @import("log");
const repo_fetch = @import("repo_fetch.zig");
const bounds_mod = @import("repo_fetch_bounds.zig");
const repo_fetch_env = @import("repo_fetch_env.zig");
const RepoFetchTarget = @import("RepoFetchTarget.zig");
const client_errors = @import("engine/client_errors.zig");

const log = logging.scoped(.fleet_runner);
const ERR_EXEC_RUNNER_FLEET_INIT = client_errors.ERR_EXEC_RUNNER_FLEET_INIT;
const ERR_EXEC_RUNNER_FLEET_RUN = client_errors.ERR_EXEC_RUNNER_FLEET_RUN;

/// Why a fetch produced no tree. Separate from `repo_fetch.Refusal` on purpose:
/// that one says the ask was not allowed, this one says the allowed ask did not
/// complete, and a child reformulating against them does different things
/// (RULE ECL). Every variant is reported to the child as a reason string and
/// nothing more — git's stderr stays daemon-side, because remote-authored bytes
/// have no business entering a model's context (RULE PRI).
pub const Failure = enum {
    /// The target directory could not be claimed; see `RepoFetchTarget.Refusal`
    /// in the log for which of the three proofs failed.
    target_unavailable,
    /// No `git` on this host. An operator problem, not a fleet one.
    git_unavailable,
    /// The token is longer than the header buffer admits — refuse rather than
    /// silently fetch unauthenticated and fail at the vendor.
    credential_unusable,
    /// A step could not be spawned or reaped.
    process_failed,
    /// The empty object store could not be created.
    init_failed,
    /// git could not fetch what was asked: unreachable remote, refused
    /// credential, or a commit the remote would not serve.
    fetch_failed,
    /// Fetched, but the head could not be checked out into a working tree.
    checkout_failed,
    /// The shared wall-clock budget elapsed.
    timed_out,
    /// The tree grew past the per-fetch byte ceiling.
    over_quota,
    /// A step's stderr was lost, so the run could no longer be bounded.
    transport_lost,

    /// A short, stable reason for the child's tool result and the log. Named
    /// rather than `@tagName` so the wire words are greppable (RULE UFS).
    pub fn reason(self: Failure) []const u8 {
        return switch (self) {
            .target_unavailable => "fetch target could not be claimed in the lease workspace",
            .git_unavailable => "no git executable on this runner host",
            .credential_unusable => "the minted credential is too large to present",
            .process_failed => "a git step could not be started or reaped",
            .init_failed => "the repository could not be initialized",
            .fetch_failed => "git could not fetch the requested commit and head",
            .checkout_failed => "the target head could not be checked out",
            .timed_out => "the fetch exceeded its time budget",
            .over_quota => "the fetch exceeded its size budget",
            .transport_lost => "the fetch could no longer be bounded and was stopped",
        };
    }
};

pub const Outcome = union(enum) {
    /// A working tree is at `RepoFetchTarget.DIR_NAME` inside the lease's
    /// workspace, detached at the target head, with the suspect commit and its
    /// parent present. The child is TOLD this name; it never supplies one.
    ready,
    failed: Failure,
};

/// Everything the caller must supply. `remote_url` is injected rather than
/// derived here so the whole execution half runs against a local `file://`
/// fixture — no network, no credential — which is what makes the depth bound and
/// the quota testable at all (the "pure core, injected effects" shape). Production
/// builds it with `repo_fetch.remoteUrl`, from the binding's spelling.
pub const Request = struct {
    /// Daemon-derived from `lease_id`. The child cannot supply one (Invariant 2).
    workspace_path: []const u8,
    approved: repo_fetch.Approved,
    remote_url: []const u8,
    /// The minted installation token, or "" for an unauthenticated remote.
    /// Borrowed for the call; never logged, never written under the target.
    token: []const u8,
    /// Absolute epoch-ms ceiling for the WHOLE sequence. The caller clamps it to
    /// the lease so a fetch can never outlive the run that asked for it.
    deadline_ms: i64,
};

/// Fetch the approved commit, its parent, and the target head into the lease's
/// own workspace. Never fails the lease: every path returns a named `Outcome`.
///
/// `alloc` backs a scratch arena for the git environment; nothing survives the
/// call (rule A4 — the arena is the ownership unit for a transient operation).
pub fn fetch(io: std.Io, alloc: std.mem.Allocator, req: Request) Outcome {
    const git = gitPath(io) orelse {
        log.err("repo_fetch_git_missing", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT });
        return .{ .failed = .git_unavailable };
    };

    var target = switch (RepoFetchTarget.claim(io, req.workspace_path)) {
        .claimed => |t| t,
        .refused => return .{ .failed = .target_unavailable },
    };
    defer target.close(io);

    var arena_state = std.heap.ArenaAllocator.init(alloc);
    defer arena_state.deinit();
    var environ = repo_fetch_env.build(arena_state.allocator(), req.remote_url, req.token) catch |err| return .{ .failed = switch (err) {
        error.CredentialUnusable => Failure.credential_unusable,
        error.OutOfMemory => Failure.process_failed,
    } };
    defer environ.deinit();

    const step_bounds: bounds_mod.Bounds = .{
        .deadline_ms = req.deadline_ms,
        .max_bytes = MAX_FETCH_BYTES,
    };

    // Refspecs live in this frame because the argv slices borrow them; both are
    // bounded by the object-id and branch ceilings `repo_fetch.decide` enforced.
    var suspect_buf: [MAX_REFSPEC_LEN]u8 = undefined;
    var head_buf: [MAX_REFSPEC_LEN]u8 = undefined;
    const suspect_spec = refspec(&suspect_buf, req.approved.commit, REF_SUSPECT);
    const head_src = if (req.approved.head.len > 0) req.approved.head else DEFAULT_HEAD_SRC;
    const head_spec = refspec(&head_buf, head_src, REF_HEAD);

    const steps = [_]Step{
        .{ .failure = .init_failed, .argv = &.{ git, "init", QUIET } },
        .{ .failure = .fetch_failed, .argv = &.{
            git,            "fetch",       QUIET,
            NO_TAGS,        NO_SUBMODULES, DEPTH_FLAG,
            req.remote_url, suspect_spec,  head_spec,
        } },
        .{ .failure = .checkout_failed, .argv = &.{ git, "checkout", QUIET, DETACH, REF_HEAD } },
    };

    for (steps) |step| {
        var stderr_buf: [STDERR_TAIL_BYTES]u8 = undefined;
        const outcome = bounds_mod.run(io, .{
            .argv = step.argv,
            .environ = &environ,
            .cwd = target.dir,
            .target = target.dir,
            .bounds = step_bounds,
        }, &stderr_buf) catch return .{ .failed = .process_failed };

        if (outcome.succeeded()) continue;
        // The tail goes to the operator's log, never to the child: it is the
        // remote's prose, and the child gets the enum's reason instead.
        log.warn("repo_fetch_step_failed", .{
            .error_code = ERR_EXEC_RUNNER_FLEET_RUN,
            .step = @tagName(step.failure),
            .stop = @tagName(outcome.stop),
            .exit_code = if (outcome.exit_code) |code| @as(i16, code) else EXIT_CODE_KILLED,
            .stderr = outcome.stderr,
        });
        return .{ .failed = switch (outcome.stop) {
            .completed => step.failure,
            .timed_out => .timed_out,
            .over_quota => .over_quota,
            .transport_lost => .transport_lost,
        } };
    }

    log.info("repo_fetch_ready", .{ .repository = req.approved.repository, .commit = req.approved.commit });
    return .ready;
}

/// One git invocation and the failure its non-zero exit means.
const Step = struct {
    failure: Failure,
    argv: []const []const u8,
};

/// `+<src>:<dst>` — forced so a re-run over a reused ref cannot fail on
/// non-fast-forward, and explicit destinations so the later steps name a ref
/// rather than parsing `FETCH_HEAD`.
fn refspec(buf: *[MAX_REFSPEC_LEN]u8, src: []const u8, dst: []const u8) []const u8 {
    // `src` cleared `repo_fetch.decide`'s ceilings (object id ≤ 64, branch ≤ 255)
    // and `dst` is a compile-time constant, so the buffer always fits and this
    // cannot fail at runtime.
    std.debug.assert(src.len <= MAX_REFSPEC_SRC_LEN);
    var end: usize = 0;
    for ([_][]const u8{ REFSPEC_FORCE, src, REFSPEC_SEPARATOR, dst }) |part| {
        @memcpy(buf[end..][0..part.len], part);
        end += part.len;
    }
    return buf[0..end];
}

/// The git executable, from an absolute-path allowlist. Never resolved through
/// the parent `$PATH` — the same trust dependency `requireAbsoluteArgv0` refuses
/// for the sandbox wrapper, for the same reason (`sandbox_args.bwrapPath` is the
/// shipped shape this mirrors, down to being pub: "does this host have the
/// binary" is a host fact other callers ask, and the fixture builder in the
/// sibling test asks it first).
pub fn gitPath(io: std.Io) ?[]const u8 {
    for (GIT_PATHS) |p| {
        std.Io.Dir.accessAbsolute(io, p, .{}) catch continue;
        return p;
    }
    return null;
}

const GIT_PATHS = [_][]const u8{ "/usr/bin/git", "/usr/local/bin/git", "/opt/homebrew/bin/git" };

const QUIET = "--quiet";
const NO_TAGS = "--no-tags";
const NO_SUBMODULES = "--no-recurse-submodules";
const DETACH = "--detach";
/// Depth 2, over two tips. Reverting `C` onto head `H` needs the trees of `C`,
/// `C^`, and `H` and no history walk, so this fetches `{C, C^} ∪ {H, H^}` — at
/// or under the three-commit floor the spec sets, and never a full clone.
const DEPTH_FLAG = "--depth=2";

/// Where the two fetched tips land. Under `refs/agentsfleet/` so they cannot
/// collide with anything the repository itself carries, and shared verbatim with
/// the checkout step and the tests (RULE UFS).
const REF_SUSPECT = "refs/agentsfleet/suspect";
const REF_HEAD = "refs/agentsfleet/head";
/// Fetched as the head when the ask named no branch: the remote's own default.
const DEFAULT_HEAD_SRC = "HEAD";

/// Ceiling on bytes under the fetch target, across every step. Half the lease's
/// nominal disk budget (`engine/types.ResourceLimits.disk_write_limit_mb`), so a
/// fetch can never consume all of it, and host cost is `worker_count ×` this.
const BYTES_PER_MIB: u64 = 1024 * 1024;
const MAX_FETCH_BYTES: u64 = 512 * BYTES_PER_MIB;

/// Head of a failed step's stderr, kept for the operator's log only.
const STDERR_TAIL_BYTES: usize = 4096;

/// Logged in place of an exit status when the step was killed on a bound rather
/// than allowed to exit — no real status exists, and 0 would read as success.
const EXIT_CODE_KILLED: i16 = -1;

/// Refspec ceilings, derived from what `repo_fetch.decide` admits: an object id
/// (≤64) or a branch name (≤255) on the left, a constant ref on the right.
const REFSPEC_FORCE = "+";
const REFSPEC_SEPARATOR = ":";
const MAX_REFSPEC_SRC_LEN: usize = 255;
const MAX_REFSPEC_LEN: usize = REFSPEC_FORCE.len + MAX_REFSPEC_SRC_LEN + REFSPEC_SEPARATOR.len + REF_SUSPECT.len;

test {
    _ = @import("repo_fetch_exec_test.zig");
}
