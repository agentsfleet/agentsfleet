//! The workspace's own library collection and its removal verb, over live stores.
//!
//! `library_entry/tests.rs` grades the values — the route rows and the cursor.
//! What only a live router reaches is everything decided from a ROW: which
//! workspace a page answers for, whether a second `DELETE` changes anything,
//! and whether the merged gallery beside it still reads the same. None of those
//! is observable without the real schema, because each is a predicate the
//! statement carries rather than a branch the handler takes.
//!
//! The fixture holds two workspaces. Only one is reachable over the router; the
//! other's rows are seeded directly, because what is being proved about it is
//! that they never appear — and a row that cannot be read through this router
//! is exactly the shape a foreign tenant's row has.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

use http::StatusCode;
use serde_json::Value;

use self::fixture::Live;
use self::harness::ERROR_CODE;
#[path = "workspace_library_entries_live/events.rs"]
mod events;
#[path = "workspace_library_entries_live/fixture.rs"]
mod fixture;

/// The subject the fixture credential authenticates as.
const SUBJECT: &str = "user_live_library_entries";
/// The upload source every onboarding here goes through — no network.
const SOURCE_KIND: &str = "upload";
/// The code a cursor this collection did not issue earns.
const CURSOR_MALFORMED: &str = "UZ-LIBRARY-001";
/// A token no walk minted.
const FOREIGN_CURSOR: &str = "not-a-cursor-this-collection-issued";
/// The family every cursor refusal belongs to, and a malformed id belongs to none.
const LIBRARY_CODE_FAMILY: &str = "UZ-LIBRARY-";
/// A path segment that is not an identifier at all.
const MALFORMED_ENTRY_ID: &str = "not-a-uuid";
/// The line a completed removal leaves behind.
const REMOVAL_EVENT: &str = "workspace_library_entry_removed";
/// What a bundle holds, none of which may reach a log line.
const BUNDLE_FIELDS: [&str; 4] = [
    "skill_markdown",
    "trigger_markdown",
    "content_hash",
    "source_ref",
];

/// Dimension 2.2 — the collection answers for one workspace and no other.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_owned_collection_answers_only_this_workspace() {
    let live = Live::start().await;
    live.seed_platform_rows(2).await;
    live.seed_foreign_entries(2).await;
    let mine = live.onboard("mine").await;

    let page = live.owned_page("").await;
    let items = items_of(&page);
    assert_eq!(items.len(), 1, "one entry, this workspace's own: {page}");
    assert_eq!(
        items
            .first()
            .and_then(|item| item.get("id"))
            .and_then(Value::as_str),
        Some(mine.as_str()),
        "and it is the one this workspace onboarded"
    );

    let refused = live
        .owned_response(&format!("?starting_after={FOREIGN_CURSOR}"))
        .await;
    assert_eq!(refused.0, StatusCode::BAD_REQUEST);
    assert_eq!(
        refused.1.get(ERROR_CODE).and_then(Value::as_str),
        Some(CURSOR_MALFORMED),
        "{}",
        refused.1
    );
    live.cleanup().await;
}

/// Dimension 2.3 — removal is idempotent, under a replay and under a race.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_delete_is_idempotent_under_replay_and_concurrency() {
    let live = Live::start().await;
    let entry = live.onboard("removed-twice").await;
    assert_eq!(live.owned_row_count().await, 1);

    let (status, body) = live.remove_response(&entry).await;
    // The body, not just the status: a refusal here carries the registry code
    // that says WHY, and a bare status turns a one-line diagnosis into a re-run
    // of a twenty-minute lane.
    assert_eq!(status, StatusCode::NO_CONTENT, "{}", text(&body));
    assert!(body.is_empty(), "204 carries no body: {}", text(&body));
    assert!(
        items_of(&live.owned_page("").await).is_empty(),
        "the follow-up read omits it"
    );

    let (replayed, body) = live.remove_response(&entry).await;
    assert_eq!(replayed, StatusCode::NO_CONTENT, "{}", text(&body));
    assert_eq!(live.owned_row_count().await, 0, "a replay changes nothing");

    let raced = live.onboard("removed-at-once").await;
    let (first, second) = tokio::join!(live.remove_response(&raced), live.remove_response(&raced));
    assert_eq!(first.0, StatusCode::NO_CONTENT, "{}", text(&first.1));
    assert_eq!(second.0, StatusCode::NO_CONTENT, "{}", text(&second.1));
    assert_eq!(live.owned_row_count().await, 0, "one row, removed once");
    live.cleanup().await;
}

/// Dimension 2.7 — the merged gallery and the onboarding verb are untouched.
///
/// The platform tier's own delete is not re-asserted here: `admin_live/
/// libraries.rs` already grades it, refusing a published row with a conflict,
/// and `admin_scope_gates.rs` grades the scope it sits behind. Both must stay
/// passing unamended, which is the assertion — this case covers the half that
/// is new, the gallery reading the same with a second collection beside it.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_gallery_and_platform_delete_are_unchanged() {
    let live = Live::start().await;
    let platform = live.seed_platform_rows(2).await;
    let onboarded = live.onboard("in-the-gallery").await;

    let before = live.gallery_ids().await;
    for id in &platform {
        assert!(
            before.contains(id),
            "the platform row still shows: {before:?}"
        );
    }
    assert!(
        before.contains(&onboarded),
        "and so does the onboarded one: {before:?}"
    );

    assert_eq!(
        live.remove_response(&onboarded).await.0,
        StatusCode::NO_CONTENT
    );
    // Compared against THIS fixture's own platform rows, never the whole
    // catalogue. `core.fleet_library` is global — a published row is visible
    // to every workspace — so a sibling case seeding its own platform rows
    // puts them in this gallery too. Asserting on the unfiltered list makes
    // the case pass or fail on which tests happen to run beside it.
    let after: Vec<String> = live
        .gallery_ids()
        .await
        .into_iter()
        .filter(|id| platform.contains(id) || *id == onboarded)
        .collect();
    assert_eq!(
        after, platform,
        "removing the tenant entry leaves the platform rows exactly as they were"
    );
    live.cleanup().await;
}

/// Dimension 2.4 — a malformed identifier is refused before any statement runs.
///
/// Graded here rather than beside the route rows because the refusal is only
/// reachable through the router: `parse_entry_id` is private to the handler,
/// and making it visible to widen a test would be the test shaping the code.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_delete_refusals_are_malformed_and_scoped() {
    let live = Live::start().await;
    let (status, body) = live.remove_response(MALFORMED_ENTRY_ID).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let body: Value = serde_json::from_slice(&body).expect("a refusal carries the envelope");
    let code = body
        .get(ERROR_CODE)
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        !code.starts_with(LIBRARY_CODE_FAMILY),
        "a malformed id mints no library code: {body}"
    );
    live.cleanup().await;
}

/// Dimension 2.6 — the removal's line carries the outcome and none of the bundle.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_removal_emits_a_scoped_event_without_bundle_content() {
    let live = Live::start().await;
    let entry = live.onboard("logged").await;

    let (events, _capturing) = events::capture();
    assert_eq!(live.remove_response(&entry).await.0, StatusCode::NO_CONTENT);
    let removed = events.named(REMOVAL_EVENT);

    assert_eq!(removed.field("library_entry_id"), Some(entry.as_str()));
    assert_eq!(
        removed.field("removed"),
        Some("true"),
        "the outcome is on it"
    );
    assert!(
        removed.carries("workspace_id"),
        "and the scope it ran under"
    );
    for field in BUNDLE_FIELDS {
        assert!(!removed.carries(field), "the line carries no {field}");
    }
    live.cleanup().await;
}

/// A response body as text, for a failure message that has to name itself.
fn text(body: &[u8]) -> String {
    String::from_utf8_lossy(body).into_owned()
}

/// Every item on one page, as the envelope carries them.
fn items_of(page: &Value) -> Vec<Value> {
    page.get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}
