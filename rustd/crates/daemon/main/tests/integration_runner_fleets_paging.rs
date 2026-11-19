//! The runner's fleets and events reads page, refuse a bound or cursor they
//! did not issue, and cut an answer to its excerpt.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly, and a step \
              indexes the JSON it was answered"
)]

use af_core::id::Uuid7;
use af_wire::fleets_verb::EXCERPT_MAX_BYTES;
use agentsfleetd::supervisor::Supervisor;
use serde_json::Value;
use url::form_urlencoded::Serializer;

use crate::steers::Crew;

/// The query that continues a walk the way the supervisor writes it: the
/// cursor form-encoded, so its `:` arrives as `%3A`.
fn continuing(cursor: &str) -> String {
    let mut query = Serializer::new(String::from("&limit=2&"));
    query.append_pair("starting_after", cursor);
    query.finish()
}

/// The page a read answered: its item ids, and the cursor for the next one.
fn page(answer: &Value, id_key: &str) -> (Vec<String>, Option<String>) {
    let ids = answer["items"]
        .as_array()
        .expect("a page carries items")
        .iter()
        .map(|item| {
            item[id_key]
                .as_str()
                .expect("an item names itself")
                .to_owned()
        })
        .collect();
    (ids, answer["next_cursor"].as_str().map(str::to_owned))
}

/// The runner's route for `tail` under `run`, presenting `fence`.
fn route(run: &str, tail: &str, fence: u64, query: &str) -> String {
    format!("/v1/runners/me/runs/{run}/{tail}?fencing_token={fence}{query}")
}

/// The answer of `event`, read from whichever of `pages` lists it, is the
/// start of `answer` cut before the character that straddles the excerpt bound.
fn assert_cut_at_a_boundary(pages: &[&Value], event: &Uuid7, answer: &str) {
    let cut = pages
        .iter()
        .filter_map(|page| page["items"].as_array())
        .flatten()
        .find(|item| item["turn_id"] == event.as_str())
        .expect("the long answer's event is on a page");
    let excerpt = cut["response_excerpt"]
        .as_str()
        .expect("an answer has an excerpt");
    assert!(excerpt.len() <= EXCERPT_MAX_BYTES, "{}", excerpt.len());
    assert!(
        answer.starts_with(excerpt),
        "the excerpt is the answer's own start"
    );
    assert_eq!(
        excerpt.len(),
        EXCERPT_MAX_BYTES - 1,
        "cut before the character that straddles the bound"
    );
}

/// The fleets list walks in pages of two to the end: four fleets, two pages,
/// the second with no cursor.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleets_read_walks_its_pages() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    for name in ["page-one", "page-two", "page-three"] {
        crew.sibling(name).await;
    }
    let (run, fence) = crew.speaking.run_seeded().await;

    let (status, first) = crew.read(&route(&run, "fleets", fence, "&limit=2")).await;
    assert_eq!(status, 200, "{first}");
    let (head, cursor) = page(&first, "fleet_id");
    let cursor = cursor.expect("two of four fleets leave a next page");
    let (status, second) = crew
        .read(&route(&run, "fleets", fence, &continuing(&cursor)))
        .await;
    assert_eq!(status, 200, "{second}");
    let (tail, last) = page(&second, "fleet_id");
    assert_eq!(
        (head.len(), tail.len(), last),
        (2, 2, None),
        "{first} {second}"
    );
    let walked: std::collections::BTreeSet<_> = head.iter().chain(&tail).collect();
    assert_eq!(
        walked.len(),
        4,
        "every fleet appears once: {head:?} {tail:?}"
    );
    crew.finish(supervisor).await;
}

/// One fleet's events walk in pages, newest first, and an answer longer than
/// the excerpt bound comes back cut at a character boundary, never inside one.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn events_read_walks_its_pages_and_cuts_answers() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let target = crew.sibling("page-events").await;
    let long = format!("a{}", "é".repeat(2 * EXCERPT_MAX_BYTES));
    let longest = crew.answered_event(&target, &long).await;
    for answer in ["first", "second"] {
        crew.answered_event(&target, answer).await;
    }
    let (run, fence) = crew.speaking.run_seeded().await;
    let events = format!("fleets/{target}/turns");

    let (status, first) = crew.read(&route(&run, &events, fence, "&limit=2")).await;
    assert_eq!(status, 200, "{first}");
    let (head, cursor) = page(&first, "turn_id");
    let cursor = cursor.expect("two of three events leave a next page");
    let (_, second) = crew
        .read(&route(&run, &events, fence, &continuing(&cursor)))
        .await;
    let (tail, last) = page(&second, "turn_id");
    assert_eq!(
        (head.len(), tail.len(), last),
        (2, 1, None),
        "{first} {second}"
    );

    assert_cut_at_a_boundary(&[&first, &second], &longest, &long);
    crew.finish(supervisor).await;
}

/// A page size outside its band, a cursor the daemon never issued and a read
/// with no fence are each a 400 with the route's sentence.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleets_reads_refuse_a_bad_bound_cursor_and_missing_fence() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let (run, fence) = crew.speaking.run_seeded().await;
    let events = format!("fleets/{}/turns", crew.run().fleet);

    for (tail, query, sentence) in [
        (
            events.as_str(),
            "&limit=0",
            "limit must be between 1 and 25",
        ),
        (
            events.as_str(),
            "&limit=26",
            "limit must be between 1 and 25",
        ),
        (
            events.as_str(),
            "&starting_after=garbage",
            "The cursor is not valid",
        ),
        (
            "fleets",
            "&starting_after=garbage",
            "The cursor is not valid",
        ),
        ("fleets", "&starting_after=%zz", "The cursor is not valid"),
        (
            events.as_str(),
            "&starting_after=%zz",
            "The cursor is not valid",
        ),
    ] {
        let (status, refused) = crew.read(&route(&run, tail, fence, query)).await;
        assert_eq!(status, 400, "{tail}{query}: {refused}");
        assert_eq!(refused["detail"], sentence, "{tail}{query}: {refused}");
    }
    let (status, refused) = crew.read(&route(&run, "fleets", fence, "&limit=0")).await;
    assert_eq!(status, 400, "{refused}");

    let unfenced = format!("/v1/runners/me/runs/{run}/fleets");
    let (status, refused) = crew.read(&unfenced).await;
    assert_eq!(status, 400, "a read with no fence is refused: {refused}");
    crew.finish(supervisor).await;
}
