//! The two reads and the one write the credential plane makes under locks it
//! holds: setup inspection under a share lock, and a rewrite of one row under
//! its own `FOR UPDATE`.
//!
//! The rewrite is the refresh-token rotation's engine, so the cases that matter
//! are the ones a rotation lives or dies on: a compare that loses leaves the
//! row alone, a vanished row is not resurrected, and a hundred racers released
//! together produce exactly one winner.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use af_core::id::Uuid7;
use af_crypto::secret::SecretBytes;
use afd_vault::{Generation, Rewritten, SecretBody, Vault};
use serde_json::Value;
use sqlx::PgConnection;
use tokio::sync::Barrier;

use crate::support::{Lane, body, named};

/// Callers released together by the contention case.
const RACERS: usize = 100;

fn owner_of(plaintext: &SecretBytes) -> String {
    let document: Value = serde_json::from_slice(plaintext.expose()).expect("a JSON object");
    document
        .get("owner")
        .and_then(Value::as_str)
        .expect("an owner field")
        .to_owned()
}

/// A handle body naming `owner`.
fn owner_body(owner: &str) -> SecretBody {
    body(&format!(r#"{{"owner":"{owner}"}}"#))
}

/// The owner the row stored under `name` names now.
async fn stored_owner(lane: &Lane, name: &str) -> String {
    let stored = lane
        .vault
        .find(&lane.workspace, name)
        .await
        .expect("the vault answers")
        .expect("the row is still there");
    owner_of(&stored)
}

async fn open_shared(
    lane: &Lane,
    connection: &mut PgConnection,
    names: &[String],
) -> Vec<Generation> {
    lane.vault
        .open_shared_on(connection, &lane.workspace, names)
        .await
        .expect("the declared set opens")
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn open_shared_on_returns_each_held_name_with_a_generation_that_moves_when_it_does() {
    let lane = Lane::create().await;
    lane.store("a", r#"{"owner":"first"}"#).await;
    lane.store("b", r#"{"owner":"second"}"#).await;
    let names = ["a".to_owned(), "b".to_owned(), "missing".to_owned()];
    let mut connection = lane.pool.acquire().await.expect("a connection");

    let before = open_shared(&lane, &mut connection, &names).await;

    let held: Vec<&str> = before.iter().map(|held| held.name.as_str()).collect();
    assert_eq!(
        held,
        ["a", "b"],
        "a name the workspace does not hold is absent"
    );
    let a_before = before.first().expect("a is held");
    let b_before = before.get(1).expect("b is held");
    assert_eq!(owner_of(&a_before.plaintext), "first");
    assert_eq!(owner_of(&b_before.plaintext), "second");
    assert!(
        before.iter().all(|held| held.generation.len() == 64
            && held.generation.bytes().all(|byte| byte.is_ascii_hexdigit())),
        "a generation is a hex digest"
    );
    assert!(
        !format!("{before:?}").contains("first"),
        "Debug names the row and its generation, never its plaintext"
    );

    lane.vault
        .replace(&lane.workspace, &named("a"), &owner_body("third"))
        .await
        .expect("a is replaced");
    let after = open_shared(&lane, &mut connection, &names).await;

    let a_after = after.first().expect("a is still held");
    let b_after = after.get(1).expect("b is still held");
    assert_ne!(a_after.generation, a_before.generation, "a changed");
    assert_eq!(b_after.generation, b_before.generation, "b did not");
    assert_eq!(owner_of(&a_after.plaintext), "third");
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn rewrite_replaces_keeps_and_reports_what_it_found() {
    let lane = Lane::create().await;
    lane.store("handle", r#"{"owner":"old"}"#).await;

    let replaced = lane
        .vault
        .rewrite(&lane.workspace, &named("handle"), |current| {
            assert_eq!(
                owner_of(current),
                "old",
                "the edit sees the stored plaintext"
            );
            Ok::<_, Infallible>(Some(body(
                r#"{"owner":"new","provider":"anthropic","api_key":"k"}"#,
            )))
        })
        .await
        .expect("the vault answers")
        .expect("infallible");

    assert_eq!(replaced, Rewritten::Replaced);
    assert_eq!(stored_owner(&lane, "handle").await, "new");
    let projection = lane.meta_columns("handle").await.expect("the row is there");
    assert_eq!(projection.kind.as_deref(), Some("provider_key"));
    assert_eq!(projection.provider.as_deref(), Some("anthropic"));
    assert_eq!(projection.has_key, Some(true));

    let kept = lane
        .vault
        .rewrite(&lane.workspace, &named("handle"), |_| {
            Ok::<_, Infallible>(None)
        })
        .await
        .expect("the vault answers")
        .expect("infallible");

    assert_eq!(kept, Rewritten::Kept);
    assert_eq!(
        stored_owner(&lane, "handle").await,
        "new",
        "declining an edit writes nothing"
    );
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn rewrite_never_resurrects_a_row_and_returns_the_edits_own_refusal() {
    let lane = Lane::create().await;
    lane.store("handle", r#"{"owner":"old"}"#).await;

    let absent = lane
        .vault
        .rewrite(&lane.workspace, &named("never-stored"), |_| {
            Ok::<_, Infallible>(Some(owner_body("ghost")))
        })
        .await
        .expect("a missing row is not a vault failure")
        .expect("infallible");
    let created = lane
        .vault
        .find(&lane.workspace, "never-stored")
        .await
        .expect("the vault answers");

    assert_eq!(absent, Rewritten::Absent);
    assert!(created.is_none(), "a rewrite creates nothing");

    let refused = lane
        .vault
        .rewrite(&lane.workspace, &named("handle"), |_| {
            Err::<Option<SecretBody>, _>("the edit declined")
        })
        .await
        .expect("the vault answers");

    assert_eq!(refused, Err("the edit declined"));
    assert_eq!(
        stored_owner(&lane, "handle").await,
        "old",
        "a refused edit writes nothing"
    );
}

/// Callers inside a rewrite now and the most there ever were at once.
#[derive(Default)]
struct Flight {
    inside: AtomicUsize,
    peak: AtomicUsize,
}

impl Flight {
    fn enter(&self) {
        let now = self.inside.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
    }

    fn leave(&self) {
        self.inside.fetch_sub(1, Ordering::SeqCst);
    }
}

/// One racer: waits for the others, then claims the row if nobody owns it yet.
async fn race(
    vault: Vault,
    workspace: Uuid7,
    barrier: Arc<Barrier>,
    flight: Arc<Flight>,
    index: usize,
) -> Rewritten {
    barrier.wait().await;
    flight.enter();
    let outcome = vault
        .rewrite(&workspace, &named("race"), |current| {
            if owner_of(current) != "nobody" {
                return Ok::<_, Infallible>(None);
            }
            Ok(Some(owner_body(&format!("racer-{index}"))))
        })
        .await
        .expect("the vault answers")
        .expect("infallible");
    flight.leave();
    outcome
}

/// The compare-and-swap a refresh rotation is: every racer reads the same
/// plaintext, and only the first through the row lock still finds it unchanged.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_hundred_concurrent_compare_and_swaps_have_exactly_one_winner() {
    let lane = Lane::create().await;
    lane.store("race", r#"{"owner":"nobody"}"#).await;
    let barrier = Arc::new(Barrier::new(RACERS));
    let flight = Arc::new(Flight::default());

    let racers: Vec<_> = (0..RACERS)
        .map(|index| {
            tokio::spawn(race(
                lane.vault.clone(),
                lane.workspace.clone(),
                Arc::clone(&barrier),
                Arc::clone(&flight),
                index,
            ))
        })
        .collect();
    let mut outcomes = Vec::new();
    for racer in racers {
        outcomes.push(racer.await.expect("the racer finishes"));
    }

    let replaced = outcomes.iter().filter(|o| **o == Rewritten::Replaced);
    let kept = outcomes.iter().filter(|o| **o == Rewritten::Kept);
    assert_eq!(
        (replaced.count(), kept.count()),
        (1, RACERS - 1),
        "{outcomes:?}"
    );
    assert!(
        flight.peak.load(Ordering::SeqCst) > 1,
        "the racers never overlapped, so nothing was contended"
    );
    assert!(
        stored_owner(&lane, "race").await.starts_with("racer-"),
        "one racer's write stood"
    );
}
