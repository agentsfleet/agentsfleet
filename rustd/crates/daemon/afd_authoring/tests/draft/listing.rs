//! Collection continuations bind creator, workspace, filter and page size.

use super::{
    fixture::{Fixture, now},
    mint,
};
use afd_authoring::types::{DraftState, ListQuery};
use afd_core::error_code::AUTHORING_CURSOR;

fn query(cursor: Option<String>) -> ListQuery {
    ListQuery {
        limit: 1,
        starting_after: cursor,
        state: None,
        linked_fleet_id: None,
        q: None,
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn list_hides_other_creators_and_refuses_changed_cursor_context() {
    let f = Fixture::create().await;
    for _ in 0..2 {
        f.drafts
            .create(f.access(), f.request(mint(), None), now())
            .await
            .expect("owner draft");
    }
    let member = f
        .drafts
        .create(f.member_access(), f.request(mint(), None), now())
        .await
        .expect("member draft");
    let first = f
        .drafts
        .list(f.access(), query(None))
        .await
        .expect("owner page");
    assert_eq!(first.items.len(), 1);
    let first_id = &first.items.first().expect("first owner draft").id;
    assert_ne!(*first_id, member.draft.id);
    let cursor = first.next_cursor.expect("second owner draft remains");
    let last = f
        .drafts
        .list(f.access(), query(Some(cursor.clone())))
        .await
        .expect("owner continuation");
    assert_eq!(last.items.len(), 1);
    assert_ne!(
        last.items.first().expect("second owner draft").id,
        *first_id
    );
    assert!(last.next_cursor.is_none());
    assert!(last.total.is_none());
    assert_cursor_binding(&f, cursor).await;
    f.cleanup().await;
}

async fn assert_cursor_binding(f: &Fixture, cursor: String) {
    let error = f
        .drafts
        .list(f.member_access(), query(Some(cursor.clone())))
        .await
        .expect_err("creator binding");
    assert_eq!(error.code(), AUTHORING_CURSOR);
    let mut changed = query(Some(cursor));
    changed.state = Some(DraftState::Editing);
    assert_eq!(
        f.drafts
            .list(f.access(), changed)
            .await
            .expect_err("filter binding")
            .code(),
        AUTHORING_CURSOR
    );
}

/// The status a saved, leasable fleet holds.
const ACTIVE: &str = "active";

/// The saved fleet's current setup revision, as the database names it.
const HEAD_REVISION: &str = "SELECT r.revision FROM fleet.fleets f \
     JOIN fleet.revisions r ON r.id = f.head_revision_id WHERE f.id = $1::uuid";

/// Links a draft to a saved fleet, as a Save does once activation succeeds.
const LINK_FLEET: &str = "UPDATE authoring.drafts SET fleet_id = $2::uuid WHERE id = $1::uuid";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_listed_chat_carries_its_candidate_and_saved_setup_revisions() {
    let f = Fixture::create().await;
    let created = f
        .drafts
        .create(f.access(), f.request(mint(), None), now())
        .await
        .expect("owner draft");
    let fleet = mint();
    let mut connection = f.connection().await;
    afd_db::test_util::FleetSeed::new(
        fleet.as_str(),
        f.workspace.as_str(),
        f.tenant.as_str(),
        ACTIVE,
    )
    .name(&format!("saved-{fleet}"))
    .native_setup()
    .insert(&mut *connection)
    .await
    .expect("the saved fleet seeds");
    let unsaved = card(&f).await;
    assert_eq!(
        unsaved.candidate_revision,
        Some(1),
        "a new chat holds its first candidate"
    );
    assert_eq!(unsaved.setup_revision, None, "no fleet is saved yet");
    assert_eq!(
        unsaved.publication_revision, None,
        "nothing is published yet"
    );

    sqlx::query(LINK_FLEET)
        .bind(created.draft.id.as_str())
        .bind(fleet.as_str())
        .execute(&mut *connection)
        .await
        .expect("the draft links its saved fleet");
    let head: i64 = sqlx::query_scalar(HEAD_REVISION)
        .bind(fleet.as_str())
        .fetch_one(&mut *connection)
        .await
        .expect("the saved fleet has a head revision");
    assert_eq!(card(&f).await.setup_revision, Some(head));
    drop(connection);
    f.cleanup().await;
}

/// The owner's only listed chat.
async fn card(f: &Fixture) -> afd_authoring::types::DraftCard {
    let mut page = f
        .drafts
        .list(f.access(), query(None))
        .await
        .expect("owner page");
    assert_eq!(page.items.len(), 1, "the owner holds one chat");
    page.items.remove(0)
}

/// Renames a chat, as an explicit title edit does.
const RENAME: &str = "UPDATE authoring.drafts SET title = $2 WHERE id = $1::uuid";

/// A whole-page search for `text`.
fn search(text: &str) -> ListQuery {
    ListQuery {
        limit: 50,
        q: Some(text.to_owned()),
        ..query(None)
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_search_matches_the_creators_titles_and_turns_and_nothing_else() {
    let f = Fixture::create().await;
    let inbox = f
        .drafts
        .create(
            f.access(),
            f.request(mint(), Some("Triage my INBOX daily")),
            now(),
        )
        .await
        .expect("owner draft");
    assert_eq!(
        inbox.draft.title, "Triage my INBOX daily",
        "the first request titles the chat"
    );
    sqlx::query(RENAME)
        .bind(inbox.draft.id.as_str())
        .bind("Morning routine")
        .execute(&mut *f.connection().await)
        .await
        .expect("the chat renames");
    f.drafts
        .create(
            f.access(),
            f.request(mint(), Some("Deploy the website")),
            now(),
        )
        .await
        .expect("owner draft");
    f.drafts
        .create(
            f.member_access(),
            f.request(mint(), Some("My inbox too")),
            now(),
        )
        .await
        .expect("member draft");
    let found = |text: &'static str| {
        let f = &f;
        async move {
            f.drafts
                .list(f.access(), search(text))
                .await
                .expect("the search reads")
                .items
                .into_iter()
                .map(|card| card.id)
                .collect::<Vec<_>>()
        }
    };

    assert_eq!(
        found("inbox").await,
        std::slice::from_ref(&inbox.draft.id),
        "a turn matches, case-insensitively, the creator's own chats only"
    );
    assert_eq!(
        found("ROUTINE").await,
        std::slice::from_ref(&inbox.draft.id),
        "the renamed title matches"
    );
    for literal in ["100%", "_"] {
        assert_eq!(
            found(literal).await,
            Vec::<afd_core::id::Uuid7>::new(),
            "a LIKE metacharacter is matched as itself: {literal}"
        );
    }
    assert_search_binds_the_cursor(&f).await;
    assert_search_bounds(&f).await;
    f.cleanup().await;
}

async fn assert_search_binds_the_cursor(f: &Fixture) {
    let page = f
        .drafts
        .list(
            f.access(),
            ListQuery {
                limit: 1,
                ..search("e")
            },
        )
        .await
        .expect("a one-row search page");
    let cursor = page.next_cursor.expect("both owner chats match `e`");
    let changed = ListQuery {
        starting_after: Some(cursor),
        ..ListQuery {
            limit: 1,
            ..search("o")
        }
    };
    assert_eq!(
        f.drafts
            .list(f.access(), changed)
            .await
            .expect_err("query binding")
            .code(),
        AUTHORING_CURSOR,
        "a continuation belongs to the search that issued it"
    );
}

async fn assert_search_bounds(f: &Fixture) {
    for refused in ["   ".to_owned(), "x".repeat(201), "in\0box".to_owned()] {
        assert_eq!(
            f.drafts
                .list(f.access(), search(&refused))
                .await
                .expect_err("an unbounded search is refused")
                .code(),
            afd_core::error_code::INVALID_REQUEST,
            "refused: {:?}",
            refused.chars().take(8).collect::<String>()
        );
    }
}
