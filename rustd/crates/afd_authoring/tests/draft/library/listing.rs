//! A Library page is one statement: every card arrives joined to its head.

use super::Case;
use afd_authoring::types::LibraryQuery;
use afd_core::test_util::trace::Capture;

/// The table a per-card follow-up read would name.
const REVISIONS: &str = "authoring.library_revisions";
/// Cards on the page: more than one, so a per-card read would show.
const CARDS: usize = 3;
/// The field only sqlx's per-statement event carries.
const SQLX_STATEMENT: &str = "db.statement";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_library_list_single_statement() {
    let mut case = Case::create().await;
    for _ in 0..CARDS {
        let draft = case.draft().await;
        case.publish(&draft).await;
    }
    let (page, events) = {
        let capture = Capture::install();
        let page = case
            .setup
            .libraries(
                case.base.access(),
                LibraryQuery {
                    limit: 100,
                    starting_after: None,
                },
                true,
            )
            .await
            .expect("owned Library page");
        (page, capture.events())
    };
    let statements = events
        .iter()
        .filter(|line| {
            line.fields.contains_key(SQLX_STATEMENT)
                && line.fields.values().any(|value| value.contains(REVISIONS))
        })
        .count();
    assert_eq!(page.items.len(), CARDS, "every published card is listed");
    assert_eq!(
        statements, 1,
        "the page and every card's head revision are one statement"
    );
    case.base.cleanup().await;
}
