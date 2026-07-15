//! Unit tests for the catalog PATCH's pure verdict (`catalog_patch.zig`).
//! Split out of the handler when the widened write pushed it past the length cap
//! (RULE FLL) — the sibling `_test.zig` shape every other handler here already
//! uses. `catalog_patch.zig` force-imports this file, so the blocks stay
//! reachable from the test root.
//!
//! The write paths (transaction, guards, races) need a database and live in
//! `catalog_patch_integration_test.zig`.
//!
//! Rationale the handler file keeps to one line each (RULE: production Zig
//! stays comment-sparse):
//!
//! - **The patch is transactional** because one body may rename, repoint,
//!   curate AND publish; without it a mid-flight failure leaves a description
//!   persisted and the fleet still a draft. Every statement is guarded in SQL
//!   and `RETURNING id`-graded, so zero rows means the row moved under us
//!   (CatalogRaced) — never "nothing to do". The identity write runs first
//!   because it is the one that nulls `content_hash`, and the visibility write
//!   is guarded on that column.
//! - **`changesSource` compares against the row as read** because the dialog
//!   echoes every field back — treating "present" as "changed" would withdraw
//!   a live fleet on a copy edit. The write re-derives the same verdict inside
//!   UPDATE_CATALOG_IDENTITY's SET list against the live row, so a race here
//!   cannot leave source and content_hash disagreeing.
//! - **The publish pre-check reads row state once**; a curate-only save skips
//!   the read and takes its 404 from the guarded writes. A body that repoints
//!   and publishes together is refused up front — it asks to publish a bundle
//!   it is discarding in the same breath.
//! - **`respondVisibilityRefused` re-reads only for existence**: the whole
//!   transaction rolled back, so a 200 would silently discard the body's other
//!   writes; the 409 names the state at refusal time, and a bundle restored by
//!   an even newer refetch does not rewrite that history.
//! - **`PatchBody.id` must never exist**: workspace installs reference the row
//!   as `platform_library_id`; moving the key would orphan every install.

const std = @import("std");
const testing = std.testing;

const catalog_patch = @import("catalog_patch.zig");
const catalog = @import("catalog.zig");
const library_store = @import("../../../fleet_library/library_store.zig");

const PatchBody = catalog_patch.PatchBody;
const changesSource = catalog_patch.changesSource;
const RowState = catalog.RowState;

fn stateOf(repo: []const u8, ref: []const u8, has_bundle: bool) RowState {
    return .{
        .source_repo = repo,
        .source_ref = ref,
        .visibility = library_store.VISIBILITY_PUBLIC,
        .has_bundle = has_bundle,
        // Editable-surface fields — only `changesSource` is exercised here, which
        // reads the source pair; the rest feed the ETag surface (covered by the
        // catalog ETag integration test).
        .name = "github-pr-reviewer",
        .description = "curated",
        .reasons_raw = "{}",
    };
}

const REPO = "agentsfleet/github-pr-reviewer";
const REF = "main";

test "changesSource: an absent source field changes nothing" {
    const body: PatchBody = .{ .description = "curated" };
    try testing.expect(!changesSource(body, stateOf(REPO, REF, true)));
}

// The dialog echoes every field back, so an operator saving a
// description alone re-sends the repository it already had. Treating "present" as
// "changed" would withdraw a live fleet on a copy edit.
test "changesSource: re-sending the SAME source is a no-op" {
    const body: PatchBody = .{ .source_repo = REPO, .source_ref = REF };
    try testing.expect(!changesSource(body, stateOf(REPO, REF, true)));
}

test "changesSource: a different repository changes the source" {
    const body: PatchBody = .{ .source_repo = "agentsfleet/other" };
    try testing.expect(changesSource(body, stateOf(REPO, REF, true)));
}

test "changesSource: a different ref changes the source" {
    const body: PatchBody = .{ .source_ref = "v2" };
    try testing.expect(changesSource(body, stateOf(REPO, REF, true)));
}

// The bundle is keyed to BOTH halves of the source, so pinning a tag on the same
// repository invalidates it just as repointing the repository does.
test "changesSource: same repo, different ref still changes the source" {
    const body: PatchBody = .{ .source_repo = REPO, .source_ref = "release" };
    try testing.expect(changesSource(body, stateOf(REPO, REF, true)));
}
