//! The registry walk: how a page ENDS, and what rides alongside its rows.
//!
//! Split from [`super::registry_page`] at the file cap, along the seam the
//! suite already had. That file grades what one ROW carries — its credential,
//! its price, whether the tenant runs on it. This grades the page's own
//! boundaries instead: where a later page resumes, what an empty registry
//! answers, and the deployment-wide default the page reports without owning.
//!
//! The fixtures are its neighbour's, imported rather than restated, so a change
//! to how a credential is seeded lands in one place.

use afd_credential::provider::Boundary;

use super::Fixture;
use super::registry_page::{BODY, CAP, CREDENTIAL, add, providers, unique_model, unique_provider};

/// A limit below the row count issues a boundary, and the next page resumes
/// STRICTLY after the last row served.
///
/// The two halves are one claim: a boundary taken from the extra row rather
/// than the last served one would skip a model, and a boundary that seeked
/// inclusively would serve it twice. So the assertion is on the union and the
/// disjointness of the two pages, not on either page alone.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_limit_below_the_row_count_resumes_strictly_after_the_last_row_served() {
    let fixture = Fixture::create().await;
    let provider = unique_provider();
    fixture
        .seed_with_shape(CREDENTIAL, BODY, Some(&provider), Some(true))
        .await;
    let store = providers(&fixture);

    let mut seeded = Vec::new();
    for _ in 0..3_u8 {
        let model = unique_model();
        fixture.seed_catalogue(&provider, &model, CAP).await;
        seeded.push(add(&store, &fixture, &model).await.id);
    }

    let first = store
        .registry_page(&fixture.tenant, 2, None)
        .await
        .expect("the first page reads");
    assert_eq!(first.rows.len(), 2, "the limit is served, never the probe row");
    let boundary: Boundary = first
        .next
        .clone()
        .expect("a third row exists, so a later page does");

    let second = store
        .registry_page(&fixture.tenant, 2, Some(&boundary))
        .await
        .expect("the second page reads");
    assert_eq!(second.rows.len(), 1, "one row is left");
    assert!(
        second.next.is_none(),
        "nothing follows the last row, so no boundary is issued"
    );

    let mut walked: Vec<_> = first
        .rows
        .iter()
        .chain(second.rows.iter())
        .map(|row| row.entry.id.as_str().to_owned())
        .collect();
    walked.sort();
    let mut expected: Vec<_> = seeded.iter().map(|id| id.as_str().to_owned()).collect();
    expected.sort();
    assert_eq!(
        walked, expected,
        "the walk serves every entry exactly once — no skip, no repeat"
    );

    fixture.cleanup().await;
}

/// The page reports exactly the platform default the store's own read answers
/// — it neither invents one nor drops one.
///
/// # Why this seeds nothing
///
/// `core.platform_provider_defaults` has no tenant column and the read is
/// `WHERE active = true ... LIMIT 1`, so "the active default" is a fact about
/// the whole deployment. This lane shares one database and its suites run in
/// PARALLEL — `afd_tenant`, `afd_fleet` and `agentsfleetd` each publish an
/// `anthropic`/`claude-fixture` row of their own — so a test that seeded its
/// own default would both be unable to assert it won the `LIMIT 1`, and would
/// widen the window in which a sibling asserting the same thing loses it.
///
/// So the claim is narrowed to the half that is genuinely this page's own and
/// needs no write: the page composes its default from that read, and reports it
/// unchanged. Both-absent is a real pass, not a vacuous one — a deployment with
/// no active default must show a page with none, which is the flag the Models
/// screen gates its "switch to default" action on.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_page_reports_the_active_platform_default_the_store_reads() {
    let fixture = Fixture::create().await;
    let store = providers(&fixture);

    let read = store
        .platform_default()
        .await
        .expect("the default read answers");
    let page = store
        .registry_page(&fixture.tenant, 25, None)
        .await
        .expect("the page reads");

    match (read, page.platform_default.as_ref()) {
        (Some(direct), Some(on_page)) => {
            assert_eq!(
                on_page.default.provider, direct.provider,
                "the page reports the default the store read, not one of its own"
            );
            assert_eq!(on_page.default.model, direct.model);
            assert_eq!(
                on_page.default.context_cap_tokens,
                direct.context_cap_tokens
            );
        }
        (None, None) => {
            assert!(
                page.platform_default.is_none(),
                "no active default means no default on the page"
            );
        }
        (direct, on_page) => panic!(
            "the page and the store disagree on whether a default exists: \
             store={:?}, page={:?}",
            direct.is_some(),
            on_page.is_some()
        ),
    }

    fixture.cleanup().await;
}

/// A tenant that has registered nothing gets an empty page, not a refusal — and
/// no boundary, because there is no later page to resume.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_empty_registry_answers_an_empty_page_with_no_boundary() {
    let fixture = Fixture::create().await;
    let store = providers(&fixture);

    let page = store
        .registry_page(&fixture.tenant, 25, None)
        .await
        .expect("an empty registry is a page, not a failure");

    assert!(page.rows.is_empty());
    assert!(page.next.is_none());

    fixture.cleanup().await;
}
