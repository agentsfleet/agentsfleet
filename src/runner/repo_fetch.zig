//! repo_fetch.zig — what the daemon will fetch for a child, and everything it
//! refuses before reaching the network.
//!
//! The repairer needs a real working tree: `git revert` performs a three-way
//! merge and fails cleanly, where reconstructing a revert from the vendor's REST
//! API silently destroys unrelated work whenever the base has moved. So the
//! child asks its daemon mid-run — the shipped `MintHook` pattern with a
//! different payload — and the daemon fetches the suspect commit, its parent,
//! and the target head into the lease's own workspace. The child then reverts
//! against a real tree with no network and no credential.
//!
//! Every field of the ask is MODEL-authored, so this module treats all of it as
//! hostile input. It is deliberately pure: the whole refusal surface is decided
//! with no filesystem, no subprocess, and no network, which is what lets the
//! "refused before any network call" property be a unit test rather than a
//! packet capture.
//!
//! Four refusals, and each exists because something real gets past the others:
//!
//!   * NO BINDING — a fleet that declared no `repositories` mints no token, so a
//!     fetch could not authenticate anyway; refusing here makes the two rings
//!     fail closed together instead of one discovering it at the vendor.
//!   * OUTSIDE BINDING — the GitHub mint reduces `owner/repo` to the bare name,
//!     because that is how installation tokens scope. So a token minted for
//!     `otherorg/payments` genuinely reaches `<installed-org>/payments`, and the
//!     vendor cannot refuse what it cannot distinguish. This check on the FULL
//!     `owner/repo` is the only ring that can.
//!   * MALFORMED REPOSITORY — nothing upstream validates the spelling: the
//!     binding is authored text and the ask is model text. An unvalidated name
//!     reaches a URL and a path, so separators, traversal, and control bytes are
//!     refused rather than escaped.
//!   * MALFORMED COMMIT — a revert must name one immutable object. A branch or
//!     tag resolves to different bytes over time, so accepting one would let the
//!     approved intent and the fetched tree disagree.

const std = @import("std");
const execution_policy = @import("contract").execution_policy;

/// What the child asked its daemon to fetch. Every field is model-authored text
/// arriving over the pipe; nothing here has been validated yet.
pub const Ask = struct {
    /// Full `owner/repo` — never a bare name, never a URL.
    repository: []const u8,
    /// The suspect commit to revert, as a full object id.
    commit: []const u8,
    /// The branch the revert targets. Empty asks for the remote's default head.
    head: []const u8 = "",
};

/// Why a fetch was refused. Each variant is a distinct authoring or scoping
/// mistake, because "rejected" alone leaves the model nothing to reformulate
/// against and leaves an operator nothing to read in the activity stream.
pub const Refusal = enum {
    /// The fleet declared no `repositories` binding, so no token exists either.
    no_binding,
    /// Well-formed, but not a repository this fleet declared.
    outside_binding,
    /// Not a well-formed `owner/repo`.
    malformed_repository,
    /// Not a full, lowercase, hexadecimal object id.
    malformed_commit,
    /// Not a well-formed branch name.
    malformed_head,

    /// A short, stable reason string for the child's tool result and the log.
    /// Named rather than `@tagName` so the wire words are greppable (RULE UFS).
    pub fn reason(self: Refusal) []const u8 {
        return switch (self) {
            .no_binding => "fleet declares no repositories binding",
            .outside_binding => "repository is outside the fleet's binding",
            .malformed_repository => "repository must be a well-formed owner/repo",
            .malformed_commit => "commit must be a full lowercase object id",
            .malformed_head => "head must be a well-formed branch name",
        };
    }
};

/// An ask that cleared every check, carrying the binding's own spelling of the
/// repository rather than the model's. Two spellings that differ only in case
/// name one repository at the vendor, and the authored one is the one an
/// operator declared — so it is the one that reaches the URL, the workspace
/// path, and the log.
pub const Approved = struct {
    repository: []const u8,
    commit: []const u8,
    head: []const u8,
    access: execution_policy.RepositoryAccess,
};

pub const Verdict = union(enum) {
    approved: Approved,
    refused: Refusal,
};

/// Decide one fetch ask against the lease's binding. Pure — no filesystem, no
/// subprocess, no network — so a refusal provably precedes any egress.
pub fn decide(binding: ?execution_policy.RepositoryBinding, ask: Ask) Verdict {
    const bound = binding orelse return .{ .refused = .no_binding };
    if (!isWellFormedRepository(ask.repository)) return .{ .refused = .malformed_repository };
    if (!isObjectId(ask.commit)) return .{ .refused = .malformed_commit };
    if (ask.head.len > 0 and !isWellFormedBranch(ask.head)) return .{ .refused = .malformed_head };

    const declared = matchBinding(bound.repositories, ask.repository) orelse
        return .{ .refused = .outside_binding };
    return .{ .approved = .{
        .repository = declared,
        .commit = ask.commit,
        .head = ask.head,
        .access = bound.access,
    } };
}

/// The declared spelling of `requested`, or null when the binding does not name
/// it. Case-insensitive, because an owner and a repository name are
/// case-insensitively unique at the vendor — `acme/Payments` and `acme/payments`
/// ARE one repository, so a case-sensitive compare would refuse a legitimate ask
/// while never narrowing the set a case-insensitive one admits.
fn matchBinding(declared: []const []const u8, requested: []const u8) ?[]const u8 {
    for (declared) |entry| {
        if (std.ascii.eqlIgnoreCase(entry, requested)) return entry;
    }
    return null;
}

/// True for a full `owner/repo` with exactly one separator and two well-formed
/// segments. Rejects a bare name, a URL, a nested path, and anything carrying a
/// traversal or a control byte — the string becomes part of both a remote URL
/// and a directory path, so it is checked once, here, rather than escaped twice.
fn isWellFormedRepository(name: []const u8) bool {
    if (name.len == 0 or name.len > MAX_REPOSITORY_LEN) return false;
    const slash = std.mem.indexOfScalar(u8, name, SEGMENT_SEPARATOR) orelse return false;
    // Exactly one separator: a second means a path, not a repository.
    if (std.mem.indexOfScalarPos(u8, name, slash + 1, SEGMENT_SEPARATOR) != null) return false;
    return isWellFormedSegment(name[0..slash]) and isWellFormedSegment(name[slash + 1 ..]);
}

/// True for one owner or repository segment: non-empty, within the vendor's
/// length ceiling, built only from characters the vendor permits, and neither
/// starting with a dot nor being a traversal. A leading dot would hide the
/// directory the fetch lands in; `..` would climb out of it.
fn isWellFormedSegment(segment: []const u8) bool {
    if (segment.len == 0 or segment.len > MAX_SEGMENT_LEN) return false;
    if (segment[0] == '.') return false;
    for (segment) |c| {
        if (!std.ascii.isAlphanumeric(c) and std.mem.indexOfScalar(u8, SEGMENT_PUNCTUATION, c) == null)
            return false;
    }
    return true;
}

/// True for a full object id: 40 hexadecimal characters (SHA-1) or 64
/// (SHA-256), lowercase. An abbreviated id is refused because it is ambiguous by
/// construction, and a branch or tag is refused because it names different bytes
/// at different times — the approval named one revert, so one object is fetched.
fn isObjectId(id: []const u8) bool {
    if (id.len != OBJECT_ID_SHA1_LEN and id.len != OBJECT_ID_SHA256_LEN) return false;
    for (id) |c| {
        if (!std.ascii.isHex(c) or std.ascii.isUpper(c)) return false;
    }
    return true;
}

/// True for a branch name safe to hand to git as a refspec. This is narrower
/// than git's own rules on purpose: a name that cannot be mistaken for an option
/// (`-` prefix), cannot traverse (`..`), and carries no separator, whitespace,
/// control byte, or refspec metacharacter needs no quoting anywhere downstream.
fn isWellFormedBranch(name: []const u8) bool {
    if (name.len == 0 or name.len > MAX_BRANCH_LEN) return false;
    if (name[0] == '-' or name[0] == '.') return false;
    if (std.mem.indexOf(u8, name, TRAVERSAL) != null) return false;
    for (name) |c| {
        if (!std.ascii.isAlphanumeric(c) and std.mem.indexOfScalar(u8, BRANCH_PUNCTUATION, c) == null)
            return false;
    }
    return true;
}

/// Separator between the owner and the repository name.
const SEGMENT_SEPARATOR: u8 = '/';
/// Non-alphanumeric characters the vendor permits inside an owner or repository
/// name. Notably excludes `/`, whitespace, and every shell and URL metacharacter.
const SEGMENT_PUNCTUATION: []const u8 = "-_.";
/// Non-alphanumeric characters permitted inside a branch name. `/` is excluded
/// so a branch can never be spelled as a path.
const BRANCH_PUNCTUATION: []const u8 = "-_.";
const TRAVERSAL: []const u8 = "..";

/// The vendor's ceiling on one owner or repository name, and on the pair.
const MAX_SEGMENT_LEN: usize = 100;
const MAX_REPOSITORY_LEN: usize = MAX_SEGMENT_LEN * 2 + 1;
/// A branch long enough for any real convention, short enough to bound the
/// refspec buffer the fetch builds.
const MAX_BRANCH_LEN: usize = 255;

/// Hexadecimal object-id widths: SHA-1 today, SHA-256 for repositories that have
/// migrated. Both are accepted; nothing shorter is.
const OBJECT_ID_SHA1_LEN: usize = 40;
const OBJECT_ID_SHA256_LEN: usize = 64;

test {
    _ = @import("repo_fetch_test.zig");
}
