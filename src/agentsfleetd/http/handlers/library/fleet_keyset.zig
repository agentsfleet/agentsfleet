//! The Fleet gallery's compound sort order and seek predicate (§3).
//!
//! ## Why a rank rather than the tier name
//!
//! The gallery merges two tables — `core.fleet_library` (platform) and
//! `core.tenant_fleet_library` (tenant) — into one page. Merging them requires a
//! total order across both, and the tier participates in it: platform entries
//! sort before tenant entries at the same timestamp.
//!
//! Ordering on the tier LABEL would make that order alphabetical, which is
//! coincidence, not intent: `platform` < `tenant` happens to be the order we
//! want, and would silently invert the day a third tier named `curated` appears.
//! `tier_rank` states the intent as a number — platform 0, tenant 1 — so the
//! comparison says what it means and a new tier picks its own position.
//!
//! The owner declined renaming the wire field `visibility` to `tier`
//! (`docs/REST_API_DESIGN_GUIDELINES.md` §9 forbids v1 field renames), so the
//! rank is an internal sort key only. It never appears in a response body.
//!
//! ## The order, and the predicate that resumes it
//!
//!     created_at DESC, tier_rank ASC, id DESC
//!
//! A keyset page resumes from the last row it returned, so the predicate must
//! select exactly the rows STRICTLY AFTER that position in this order. Written
//! out, that is the three-part disjunction in `follows` — and the reason it is a
//! function here rather than three lines of SQL repeated at each call site is
//! that the two must agree exactly. A predicate that disagrees with its ORDER BY
//! does not error; it silently skips or repeats rows at every page boundary,
//! which is the single most common keyset-pagination bug.

const std = @import("std");

const pagination = @import("../../pagination.zig");

/// The persisted and wire spellings of each tier. Named once so `fromLabel` and
/// `label` cannot drift into disagreeing — a parse that accepts a spelling the
/// renderer never emits is a round-trip that silently loses rows.
pub const LABEL_PLATFORM = "platform";
pub const LABEL_TENANT = "tenant";

/// Which library a gallery row came from. The numeric rank is the sort position,
/// not an encoding of the name.
pub const Tier = enum(u8) {
    platform = 0,
    tenant = 1,

    pub fn rank(self: Tier) u8 {
        return @intFromEnum(self);
    }

    /// Parse the persisted/wire spelling. Returns null for anything else — an
    /// unknown tier must not silently become `platform` and leak entries from a
    /// library the caller cannot read.
    pub fn fromLabel(text: []const u8) ?Tier {
        if (std.mem.eql(u8, text, LABEL_PLATFORM)) return .platform;
        if (std.mem.eql(u8, text, LABEL_TENANT)) return .tenant;
        return null;
    }

    /// Map a persisted sort rank back to its tier. Returns null for anything
    /// outside the enum — a rank the projection cannot name must fail the read
    /// rather than reach a response body as a bare number.
    pub fn fromRank(value: i32) ?Tier {
        return switch (value) {
            0 => .platform,
            1 => .tenant,
            else => null,
        };
    }

    pub fn label(self: Tier) []const u8 {
        return switch (self) {
            .platform => LABEL_PLATFORM,
            .tenant => LABEL_TENANT,
        };
    }
};

/// A row's position in the gallery order. `id` is compared bytewise, matching
/// the `COLLATE "C"` the query pins — a locale-sensitive collation would order
/// the page differently than this predicate resumes it.
pub const Position = struct {
    created_at: i64,
    tier_rank: u8,
    id: []const u8,
};

/// The gallery cursor payload. Field order is the canonical JSON key order, and
/// `pagination.decode` enforces it by re-encoding.
///
/// `q` was carried here to bind a cursor to the search that issued it, until the
/// parameter was retired. `workspace_uuid` still binds the cursor to its tenant,
/// which is the isolation-relevant half; nothing else varied the set.
pub const Cursor = struct {
    v: u8 = pagination.CURSOR_VERSION,
    created_at: i64,
    tier_rank: u8,
    id: []const u8,
    workspace_uuid: []const u8,
    limit: u32,
};

/// Total order over gallery rows: `created_at DESC, tier_rank ASC, id DESC`.
///
/// Returns how `a` sorts relative to `b`: `.lt` means `a` comes FIRST on the
/// page. Exposed so a test can assert the ordering directly instead of
/// inferring it from which rows a query happened to return.
pub fn order(a: Position, b: Position) std.math.Order {
    // created_at DESCENDING — newer first, so a LARGER timestamp sorts earlier.
    if (a.created_at != b.created_at) {
        return if (a.created_at > b.created_at) .lt else .gt;
    }
    // tier_rank ASCENDING — platform (0) before tenant (1).
    if (a.tier_rank != b.tier_rank) {
        return if (a.tier_rank < b.tier_rank) .lt else .gt;
    }
    // id DESCENDING, bytewise.
    return switch (std.mem.order(u8, a.id, b.id)) {
        .gt => .lt,
        .lt => .gt,
        .eq => .eq,
    };
}

/// Does `pos` fall strictly after `cursor` in the gallery order?
///
/// This is the seek predicate, and it must mirror `order` exactly:
///
///     created_at <  c.created_at
///  OR (created_at =  c.created_at AND tier_rank >  c.tier_rank)
///  OR (created_at =  c.created_at AND tier_rank =  c.tier_rank AND id < c.id)
///
/// Note each comparison's direction follows its sort direction: `created_at` and
/// `id` descend, so "after" means SMALLER; `tier_rank` ascends, so "after" means
/// LARGER. Getting one of those backwards is the bug this function exists to
/// contain to a single place.
///
/// Strictly after, never equal: an inclusive boundary repeats the row the
/// previous page ended on, which is the other classic keyset failure.
pub fn follows(pos: Position, cursor: Position) bool {
    return order(cursor, pos) == .lt;
}
