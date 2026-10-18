//! A Library page is one statement: every card arrives joined to its head.

use super::Case;
use afd_authoring::types::LibraryQuery;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tracing::field::{Field, Visit};
use tracing_subscriber::{Layer, layer::Context, prelude::*};

/// The table a per-card follow-up read would name.
const REVISIONS: &str = "authoring.library_revisions";
/// Cards on the page: more than one, so a per-card read would show.
const CARDS: usize = 3;

/// Counts the statements sqlx reports that name [`REVISIONS`].
#[derive(Clone, Default)]
struct Statements(Arc<AtomicUsize>);

/// Whether one sqlx query event's text names [`REVISIONS`].
#[derive(Default)]
struct Names(bool);

impl Visit for Names {
    fn record_str(&mut self, _field: &Field, value: &str) {
        self.0 |= value.contains(REVISIONS);
    }

    fn record_debug(&mut self, _field: &Field, value: &dyn std::fmt::Debug) {
        self.0 |= format!("{value:?}").contains(REVISIONS);
    }
}

impl<S: tracing::Subscriber> Layer<S> for Statements {
    fn on_event(&self, event: &tracing::Event<'_>, _context: Context<'_, S>) {
        if event.metadata().target() != "sqlx::query" {
            return;
        }
        let mut names = Names::default();
        event.record(&mut names);
        if names.0 {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_library_list_single_statement() {
    let mut case = Case::create().await;
    for _ in 0..CARDS {
        let draft = case.draft().await;
        case.publish(&draft).await;
    }
    let statements = Statements::default();
    let subscriber = tracing_subscriber::registry().with(statements.clone());
    let page = {
        let _guard = tracing::subscriber::set_default(subscriber);
        case.setup
            .libraries(
                case.base.access(),
                LibraryQuery {
                    limit: 100,
                    starting_after: None,
                },
                true,
            )
            .await
            .expect("owned Library page")
    };
    assert_eq!(page.items.len(), CARDS, "every published card is listed");
    assert_eq!(
        statements.0.load(Ordering::SeqCst),
        1,
        "the page and every card's head revision are one statement"
    );
    case.base.cleanup().await;
}
