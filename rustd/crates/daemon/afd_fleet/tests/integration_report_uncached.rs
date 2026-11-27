//! A report whose kept run facts name no catalogue version prices its lines
//! from an uncached read of the price book, and still settles the run.
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_db::config::DbRole;
use afd_db::test_util::statements::{self, Window as Counted};

use crate::report_seed::held_in;
use crate::rows::Row;
use crate::seed::{MODEL, PROVIDER};
use crate::support::Fixtures;

/// A fragment of the price book read, `afd_billing`'s `select_price_book.sql`.
const PRICE_BOOK_READ: &str = "p.input_nanos_per_mtok";

// Value: protects=a session's kept facts that never learned a catalogue version
// still settle the run, priced from the book as it stands; fails_when=a kept run
// with no version faults the report, or prices without reading a book;
// why_new=the uncached arm had no case; seam=report_with over live Postgres, the
// kept facts read from the run row with their version cleared.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_report_whose_kept_facts_name_no_catalogue_version_reads_the_book_and_settles() {
    let held = held_in(Fixtures::create_isolated().await).await;
    held.fixtures.seed_model_rate(PROVIDER, MODEL).await;
    let mut kept = held
        .runs
        .load_for_report(&held.issued.run_id, &held.runner)
        .await
        .expect("the run loads")
        .expect("the run is this runner's");
    kept.catalogue_version = None;
    let owner = held.fixtures.open(DbRole::Migrator).await;
    statements::install(&owner).await;
    let request = crate::integration_parked_sandbox::report::processed(&held, None);
    let mut untimed = af_observability::verb_span::VerbClock::untimed();

    let window = Counted::open(&owner).await;
    held.fixtures
        .plane()
        .report_with(&held.runner, &request, held.now, Some(kept), &mut untimed)
        .await
        .expect("the report settles the run");
    let ran = window.close().await;

    assert_eq!(
        ran.calls_containing(PRICE_BOOK_READ),
        1,
        "the book is read once, uncached: {:?}",
        ran.statements()
    );
    assert_eq!(
        held.fixtures
            .text(Row::Run(&held.issued.run_id), "status")
            .await
            .as_deref(),
        Some(afd_state::sql::RUN_STATUS_REPORTED)
    );
    let models: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT model FROM billing.usage_lines WHERE work_id = $1")
            .bind(&held.run_key)
            .fetch_all(&mut *held.fixtures.database.acquire().await.expect("observer"))
            .await
            .expect("models");
    assert_eq!(
        models,
        [MODEL],
        "the lines are priced under the run's model"
    );
    owner.close().await;
    held.fixtures.cleanup().await;
}
