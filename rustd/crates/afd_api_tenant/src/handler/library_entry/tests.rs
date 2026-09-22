//! The route rows this collection is mounted under, and the cursor it resumes
//! from.
//!
//! Both are decisions about values, so neither needs a pool. The verbs
//! themselves belong to the router suite, because what they add is a store and
//! an ownership extractor.
//!
//! # Why the route rows are asserted here at all
//!
//! Because two of the four registration places fail SILENTLY when skipped.
//! `docs/REST_API_DESIGN_GUIDELINES.md` §7 names them: a variant missing from
//! `ALL` compiles and is never mounted, and a route with no handler arm answers
//! 404 by design. Neither is a build failure, and both look exactly like a
//! working endpoint until someone calls it.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use afd_core::paging::struct_cursor;
use afd_http::route::{Verb, WorkspaceRoute};

use super::*;

/// The workspace every token below is minted under.
const WORKSPACE: &str = "019329c5-0000-7000-8000-0000000000b1";

/// A workspace that minted none of them.
const FOREIGN_WORKSPACE: &str = "019329c5-0000-7000-8000-0000000000b2";

/// The page size every token below is minted under.
const LIMIT: u32 = 25;

/// The onboarding instant the rendered-cursor cases mint their boundary from.
const PAGE_AT: i64 = 1_725_000_000_500;

/// The entry identifier that boundary names.
const PAGE_ID: &str = "0195b4ba-8d3a-7f13-8abc-cd0000000009";

/// Dimension 2.1 — the pair mirrors `ModelEntries` / `ModelEntry`.
///
/// The collection reads and the item removes; neither carries the other's verb.
/// A `POST` on the collection would be a second spelling of onboarding, which
/// already has a shipped path.
#[test]
fn test_library_entry_routes_mirror_the_model_entry_pair() {
    let entries = WorkspaceRoute::LibraryEntries.meta();
    assert_eq!(
        entries.template,
        "/v1/workspaces/{workspace_id}/library-entries"
    );
    assert_eq!(WorkspaceRoute::LibraryEntries.verbs(), &[Verb::Get]);

    let entry = WorkspaceRoute::LibraryEntry.meta();
    assert_eq!(
        entry.template,
        "/v1/workspaces/{workspace_id}/library-entries/{entry_id}"
    );
    assert_eq!(WorkspaceRoute::LibraryEntry.verbs(), &[Verb::Delete]);
}

/// Both rows are walked by `Route::all()`.
///
/// The silent one. A variant missing from `ALL` compiles, mounts nothing, and
/// answers 404 with nothing failing anywhere.
#[test]
fn both_rows_are_in_the_walked_set() {
    for route in [WorkspaceRoute::LibraryEntries, WorkspaceRoute::LibraryEntry] {
        assert!(
            WorkspaceRoute::ALL.contains(&route),
            "{route:?} is tabled but never mounted"
        );
    }
}

/// The gallery beside this collection is untouched.
///
/// Part of Dimension 2.7, at the one tier a unit test can reach: the merged
/// gallery keeps its path and both its verbs, so adding a collection did not
/// move the endpoint `install --library` resolves against.
#[test]
fn the_gallery_row_is_unchanged() {
    assert_eq!(
        WorkspaceRoute::FleetLibrary.meta().template,
        "/v1/workspaces/{workspace_id}/fleet-libraries"
    );
    assert_eq!(
        WorkspaceRoute::FleetLibrary.verbs(),
        &[Verb::Get, Verb::Post]
    );
}

/// A token minted for this walk resumes it.
#[test]
fn a_token_this_walk_minted_resumes_it() {
    let token = minted(1_725_000_000_000, "0195b4ba-8d3a-7f13-8abc-cd0000000002");
    let resumed = resume(&token, WORKSPACE, LIMIT).expect("the token is this walk's");
    let boundary = resumed.expect("a token always names a boundary");
    assert_eq!(boundary.created_at_ms, 1_725_000_000_000);
    assert_eq!(boundary.id, "0195b4ba-8d3a-7f13-8abc-cd0000000002");
}

/// A token minted in another workspace does not seek inside this one.
///
/// The security boundary of this file, and it is a string comparison. Without
/// it, a caller who owns workspace B and holds a token from workspace A would
/// resume A's order inside B's rows — the statements would still only return
/// B's, but the PAGE would start wherever A's boundary fell, so B's own first
/// entries would be skipped silently.
#[test]
fn a_foreign_workspace_token_is_refused() {
    let token = minted(1, "0195b4ba-8d3a-7f13-8abc-cd0000000002");
    resume(&token, FOREIGN_WORKSPACE, LIMIT)
        .expect_err("a token from another workspace must not resume here");
}

/// A token minted under a different page size is refused.
///
/// The boundary is only meaningful against the page size that produced it: the
/// same cursor spent at a different limit lands between rows the previous page
/// never served.
#[test]
fn a_token_from_a_different_page_size_is_refused() {
    let token = minted(1, "0195b4ba-8d3a-7f13-8abc-cd0000000002");
    resume(&token, WORKSPACE, LIMIT + 1)
        .expect_err("a token minted under another page size must not resume here");
}

/// No cursor at all is the first page, not a refusal.
#[test]
fn an_absent_cursor_is_the_first_page() {
    for raw in ["", "limit=25", "starting_after="] {
        assert_eq!(
            resume_from(raw, WORKSPACE, LIMIT).expect("an absent cursor is not an error"),
            None
        );
    }
}

/// A token this collection did not issue is refused rather than half-read.
#[test]
fn a_token_this_collection_did_not_issue_is_refused() {
    for token in ["not-a-cursor", "eyJ2IjoxfQ"] {
        resume(token, WORKSPACE, LIMIT)
            .expect_err("a token this collection did not mint must not resume here");
    }
}

/// A cursor for this walk, rendered as a caller would receive it.
fn minted(created_at: i64, id: &str) -> String {
    struct_cursor::render(&Cursor {
        v: struct_cursor::VERSION,
        created_at,
        id: id.to_owned(),
        workspace_uuid: WORKSPACE.to_owned(),
        limit: LIMIT,
    })
}

/// [`super::list::resume_from`] over a query string carrying `token`.
fn resume(
    token: &str,
    workspace: &str,
    limit: u32,
) -> Result<Option<afd_library::EntryPosition>, crate::handler::Refusal> {
    resume_from(&format!("starting_after={token}"), workspace, limit)
}

/// The end of the walk renders no token, so a caller cannot ask for a page
/// that does not exist.
///
/// [`super::list::rendered`] mints `next_cursor` from `page.next`, and an
/// exhausted page carries `None` there. `total` is unconditionally `None` —
/// counting a keyset page costs the scan the pagination exists to avoid.
#[test]
fn an_exhausted_page_renders_no_token() {
    let page = afd_library::OwnedPage {
        items: vec![owned_entry(PAGE_AT, PAGE_ID)],
        next: None,
    };

    let response = rendered(&page, WORKSPACE, LIMIT);

    assert!(
        response.next_cursor.is_none(),
        "a walk with nothing after it must not offer a token"
    );
    assert!(response.total.is_none(), "a keyset page never counts");
    assert_eq!(response.items.len(), 1);
}

/// A page with more behind it renders a token this same walk accepts.
///
/// The cursor is minted in [`super::list::rendered`] and read back by
/// [`super::list::resume_from`]; until now only the reading end was under
/// test, so a renderer that minted a token no parser here would take —
/// wrong workspace, wrong limit, wrong version — would have gone green.
/// Asserting the round trip is what makes the pair honest.
#[test]
fn a_rendered_token_resumes_the_walk_that_minted_it() {
    let page = afd_library::OwnedPage {
        items: vec![owned_entry(PAGE_AT, PAGE_ID)],
        next: Some(afd_library::EntryPosition {
            created_at_ms: PAGE_AT,
            id: PAGE_ID.to_owned(),
        }),
    };

    let response = rendered(&page, WORKSPACE, LIMIT);
    let token = response
        .next_cursor
        .expect("a page with more behind it offers a token");

    let boundary = resume(&token, WORKSPACE, LIMIT)
        .expect("the token this walk minted must resume this walk")
        .expect("a token always names a boundary");
    assert_eq!(boundary.created_at_ms, PAGE_AT);
    assert_eq!(boundary.id, PAGE_ID);
}

/// The minted token carries the workspace it was minted under, not the
/// caller's.
///
/// The refusing direction of the test above. Without it, a renderer that wrote
/// a constant or an empty workspace into the cursor would still round-trip
/// through the assertion above, because that one hands the same workspace back
/// in.
#[test]
fn a_rendered_token_does_not_resume_a_foreign_workspace() {
    let page = afd_library::OwnedPage {
        items: vec![owned_entry(PAGE_AT, PAGE_ID)],
        next: Some(afd_library::EntryPosition {
            created_at_ms: PAGE_AT,
            id: PAGE_ID.to_owned(),
        }),
    };

    let token = rendered(&page, WORKSPACE, LIMIT)
        .next_cursor
        .expect("a page with more behind it offers a token");

    resume(&token, FOREIGN_WORKSPACE, LIMIT)
        .expect_err("a token minted for one workspace must not resume another");
}

/// One entry, with the fields the renderer borrows.
fn owned_entry(created_at_ms: i64, id: &str) -> afd_library::OwnedEntry {
    afd_library::OwnedEntry {
        id: id.to_owned(),
        name: "incident-responder".to_owned(),
        description: "Answers pages.".to_owned(),
        source_kind: "github".to_owned(),
        source_ref: "acme/responder".to_owned(),
        content_hash: "sha256:fixture".to_owned(),
        created_at_ms,
    }
}
