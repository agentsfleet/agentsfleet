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
