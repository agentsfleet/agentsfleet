//! What a repair proposal is, and what its hash means.
//!
//! A run that diagnoses a code-shaped incident may end its report with one
//! proposal: the repository, the base commit it was written against, the files
//! it is allowed to touch, and the diff itself. This module is the pure kernel
//! around that record — parse, validate, canonicalize, hash — and performs no
//! input or output at all.
//!
//! Both ends of the approval share it deliberately. The report path validates
//! and hashes a proposal before parking it behind the approval gate; the apply
//! path recomputes the same hash over the approved bytes before writing
//! anything. Approval therefore binds bytes, not intentions — what a human
//! approved and what reaches a repository are provably identical — and one
//! implementation of "what this proposal is" keeps the two ends from drifting.

const std = @import("std");
const ec = @import("../errors/error_registry.zig");

const Sha256 = std.crypto.hash.sha2.Sha256;

/// Versioned tag on the fenced block a run's final report carries.
pub const BLOCK_KIND = "repair_proposal/1";

const FENCE = "```";
/// The exact opening a run's prompt tells it to emit. Kept as one constant so
/// the reasoning prompt in `library/incident-responder/SKILL.md` and this
/// parser can never drift into agreeing on different spellings.
pub const BLOCK_FENCE_OPEN = FENCE ++ "json " ++ BLOCK_KIND;

/// A proposal is meant to be a small, reviewable fix. Anything past these caps
/// is a design change a human should author, so the report path degrades it to
/// diagnosis-only rather than parking an unreviewable diff for approval.
pub const MAX_FILES: usize = 16;
pub const MAX_DIFF_BYTES: usize = 64 * 1024;
pub const MAX_EVIDENCE: usize = 12;
pub const MAX_PATH_BYTES: usize = 256;
pub const MAX_CAUSE_BYTES: usize = 1024;
pub const MAX_REPO_BYTES: usize = 128;

/// A full commit hash: a short hash would let two different commits satisfy
/// the same base-freshness check.
pub const BASE_SHA_LEN: usize = 40;

pub const HASH_HEX_LEN: usize = Sha256.digest_length * 2;

const BRANCH_PREFIX = "agentsfleet/repair-";
/// Text length of the version 7 universally unique identifier a stored
/// proposal carries.
const PROPOSAL_ID_LEN: usize = 36;
pub const BRANCH_NAME_MAX: usize = BRANCH_PREFIX.len + PROPOSAL_ID_LEN;

const PATH_SEPARATOR = '/';
const REPO_SEPARATOR = '/';
const PARENT_COMPONENT = "..";
const CURRENT_COMPONENT = ".";
const GIT_INTERNAL_PREFIX = ".git/";
const ASCII_FIRST_PRINTABLE = 0x20;
const ASCII_DELETE = 0x7F;

pub const Evidence = struct {
    kind: []const u8,
    ref: []const u8,
    digest: []const u8 = "",
};

/// A validated proposal. `files` is sorted during parse so the hash covers the
/// SET of allowed paths rather than the order a model happened to emit them.
/// Every field is owned by the `std.json.Parsed` that produced it — free that,
/// never this. Parsing copies, so a proposal outlives the report buffer it was
/// read from.
pub const Proposal = struct {
    repo: []const u8,
    base_sha: []const u8,
    files: [][]const u8,
    diff: []const u8,
    cause: []const u8,
    evidence: []const Evidence = &.{},
};

/// Every way a proposal block can fail to be one. The caller treats all of
/// them identically — the run stays diagnosis-only — but the distinct names
/// make the refusal greppable in a log.
pub const InvalidProposal = error{
    RepoShapeInvalid,
    BaseShaShapeInvalid,
    FileListEmpty,
    FileListTooLong,
    FilePathUnsafe,
    DiffEmpty,
    DiffTooLarge,
    CauseEmpty,
    CauseTooLong,
    EvidenceMissing,
    EvidenceTooLong,
};

/// Why an apply refused. Each variant maps to a registered error code, and the
/// Slack notice and the activity stream both carry that code so an operator can
/// follow one refusal across surfaces.
pub const Refusal = enum {
    stale_base,
    bounds_exceeded,
    duplicate,
    upstream,
    invalid_proposal,

    pub fn code(self: Refusal) []const u8 {
        return switch (self) {
            .stale_base => ec.ERR_REPAIR_STALE_BASE,
            .bounds_exceeded => ec.ERR_REPAIR_BOUNDS_EXCEEDED,
            .duplicate => ec.ERR_REPAIR_DUPLICATE,
            .upstream => ec.ERR_REPAIR_UPSTREAM_FAILED,
            .invalid_proposal => ec.ERR_REPAIR_PROPOSAL_INVALID,
        };
    }
};

pub const ExtractError = error{
    /// A report may carry at most one proposal. Two is not a choice for the
    /// daemon to make on a human's behalf.
    MultipleBlocks,
    UnterminatedBlock,
};

/// Find the proposal block in a run's final report, or null when there is none
/// — the common case, since most runs end at a diagnosis. The returned slice
/// borrows from `report_body`; `parse` copies, so it need not outlive it.
///
/// Only a fence at the start of a line opens a block, so a report quoting the
/// marker mid-sentence cannot smuggle one in.
pub fn extractBlock(report_body: []const u8) ExtractError!?[]const u8 {
    // discipline: ok — returns a borrowed view into `report_body`, not owned
    // memory, so neither ownership phrase applies. Same shape as `branchName`
    // below and `queue/constants.zig`'s stream-key formatter.
    const open_at = lineStartIndexOf(report_body, BLOCK_FENCE_OPEN, 0) orelse return null;
    const body_at = (std.mem.indexOfScalarPos(u8, report_body, open_at, '\n') orelse
        return error.UnterminatedBlock) + 1;
    const close_at = lineStartIndexOf(report_body, FENCE, body_at) orelse
        return error.UnterminatedBlock;
    if (lineStartIndexOf(report_body, BLOCK_FENCE_OPEN, close_at) != null) {
        return error.MultipleBlocks;
    }
    return report_body[body_at..close_at];
}

/// `std.mem.indexOfPos`, restricted to matches that begin a line.
fn lineStartIndexOf(haystack: []const u8, needle: []const u8, from: usize) ?usize {
    var at = from;
    while (std.mem.indexOfPos(u8, haystack, at, needle)) |found| {
        if (found == 0 or haystack[found - 1] == '\n') return found;
        at = found + 1;
    }
    return null;
}

/// Parse, validate, and canonicalize a proposal block. Caller frees via
/// `.deinit()`. Any error means the run stays diagnosis-only: no stored
/// proposal, no approval requested, the run's own result untouched.
pub fn parse(alloc: std.mem.Allocator, raw: []const u8) !std.json.Parsed(Proposal) {
    const parsed = try std.json.parseFromSlice(Proposal, alloc, raw, .{
        .ignore_unknown_fields = true,
        // Copy every string instead of aliasing `raw`. The caller reads a
        // proposal out of a run-report body it is free to release immediately,
        // and a proposal that quietly pointed into freed bytes would hash
        // whatever landed there next.
        .allocate = .alloc_always,
    });
    errdefer parsed.deinit();
    try validate(parsed.value);
    canonicalize(parsed.value);
    return parsed;
}

/// Sorting the allowlist is what makes the hash canonical over a set: two
/// proposals differing only in the order they listed the same paths are the
/// same proposal, and must hash the same.
fn canonicalize(p: Proposal) void {
    std.mem.sort([]const u8, p.files, {}, lessThanPath);
}

fn lessThanPath(_: void, a: []const u8, b: []const u8) bool {
    return std.mem.lessThan(u8, a, b);
}

pub fn validate(p: Proposal) InvalidProposal!void {
    if (!isValidRepo(p.repo)) return error.RepoShapeInvalid;
    if (p.base_sha.len != BASE_SHA_LEN or !isLowerHex(p.base_sha)) return error.BaseShaShapeInvalid;
    if (p.files.len == 0) return error.FileListEmpty;
    if (p.files.len > MAX_FILES) return error.FileListTooLong;
    for (p.files) |path| {
        if (!isSafeRepoPath(path)) return error.FilePathUnsafe;
    }
    if (p.diff.len == 0) return error.DiffEmpty;
    if (p.diff.len > MAX_DIFF_BYTES) return error.DiffTooLarge;
    if (p.cause.len == 0) return error.CauseEmpty;
    if (p.cause.len > MAX_CAUSE_BYTES) return error.CauseTooLong;
    // A proposal with nothing to cite is a guess. The grounding rule lives in
    // the bundle's prompt, but nothing enforces a prompt — this does.
    if (p.evidence.len == 0) return error.EvidenceMissing;
    if (p.evidence.len > MAX_EVIDENCE) return error.EvidenceTooLong;
}

/// The content address: SHA-256 over the repository, the base commit, the
/// sorted allowlist, and the diff. Cause and evidence are deliberately absent —
/// the hash binds what would be WRITTEN, so re-wording a justification cannot
/// invalidate an approval, while changing one byte of the diff always does.
///
/// Every field is length-framed, so no field's bytes can be shifted into its
/// neighbour to forge a colliding hash.
pub fn canonicalHashHex(p: Proposal) [HASH_HEX_LEN]u8 {
    var hasher = Sha256.init(.{});
    updateFramed(&hasher, p.repo);
    updateFramed(&hasher, p.base_sha);
    updateCount(&hasher, p.files.len);
    for (p.files) |path| updateFramed(&hasher, path);
    updateFramed(&hasher, p.diff);
    var digest: [Sha256.digest_length]u8 = undefined;
    hasher.final(&digest);
    return std.fmt.bytesToHex(digest, .lower);
}

fn updateFramed(hasher: *Sha256, bytes: []const u8) void {
    updateCount(hasher, bytes.len);
    hasher.update(bytes);
}

fn updateCount(hasher: *Sha256, n: usize) void {
    var len_buf: [@sizeOf(u64)]u8 = undefined;
    std.mem.writeInt(u64, &len_buf, n, .big);
    hasher.update(&len_buf);
}

/// A proposal describes one commit's worth of code. If the branch moved after
/// the human approved it, the approved diff no longer describes what is there.
pub fn baseIsFresh(p: Proposal, live_head_sha: []const u8) bool {
    return std.mem.eql(u8, p.base_sha, live_head_sha);
}

/// One branch per proposal, derived from its identifier rather than chosen.
/// That derivation is the idempotency key: a replayed approval computes the
/// same name, finds the branch already present, and refuses as a duplicate
/// instead of opening a second pull request.
pub fn branchName(buf: []u8, proposal_id: []const u8) ![]const u8 {
    // discipline: ok — returns a borrowed view into `buf` (bufPrint), not owned
    // memory, so neither ownership phrase applies. Same shape as
    // `queue/constants.zig`'s stream-key formatter.
    return std.fmt.bufPrint(buf, BRANCH_PREFIX ++ "{s}", .{proposal_id});
}

fn isValidRepo(repo: []const u8) bool {
    if (repo.len == 0 or repo.len > MAX_REPO_BYTES) return false;
    const sep = std.mem.indexOfScalar(u8, repo, REPO_SEPARATOR) orelse return false;
    if (std.mem.lastIndexOfScalar(u8, repo, REPO_SEPARATOR).? != sep) return false;
    const owner = repo[0..sep];
    const name = repo[sep + 1 ..];
    if (owner.len == 0 or name.len == 0) return false;
    return isPrintableAscii(repo) and std.mem.indexOfScalar(u8, repo, ' ') == null;
}

/// A path the apply is allowed to write. Rejects anything that could escape
/// the checkout or reach into the repository's own metadata: absolute paths,
/// parent traversal, empty or dot components, backslashes, and control bytes.
fn isSafeRepoPath(path: []const u8) bool {
    if (path.len == 0 or path.len > MAX_PATH_BYTES) return false;
    if (path[0] == PATH_SEPARATOR) return false;
    if (std.mem.startsWith(u8, path, GIT_INTERNAL_PREFIX)) return false;
    if (std.mem.indexOfScalar(u8, path, '\\') != null) return false;
    if (!isPrintableAscii(path)) return false;
    var it = std.mem.splitScalar(u8, path, PATH_SEPARATOR);
    while (it.next()) |component| {
        if (component.len == 0) return false;
        if (std.mem.eql(u8, component, PARENT_COMPONENT)) return false;
        if (std.mem.eql(u8, component, CURRENT_COMPONENT)) return false;
    }
    return true;
}

fn isPrintableAscii(bytes: []const u8) bool {
    for (bytes) |b| {
        if (b < ASCII_FIRST_PRINTABLE or b >= ASCII_DELETE) return false;
    }
    return true;
}

fn isLowerHex(bytes: []const u8) bool {
    for (bytes) |b| {
        const ok = (b >= '0' and b <= '9') or (b >= 'a' and b <= 'f');
        if (!ok) return false;
    }
    return true;
}

test {
    _ = @import("repair_proposal_test.zig");
}
