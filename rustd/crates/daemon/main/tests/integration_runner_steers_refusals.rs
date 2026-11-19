//! The steers that write nothing: the wrong target, a key a person
//! took, and a sender no longer installed; and a fleet's steer is never read
//! as a person's.
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

use af_crypto::entropy::Entropy;
use afd_tenant::preference::Preferences;
use afd_turns::Admissions;
use afd_turns::view::{Filter, History, Steer, prefix_to_like, steer_actor};
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;

use crate::steers::{ARCHIVED, Crew, INSTALLING};

/// The person a direct message comes from.
const PERSON: &str = "user_fixture_person";

/// How many characters of the digest a fleet's operation id keeps.
const KEY_DIGEST_HEX: usize = 32;

/// The sender itself, a fleet of another workspace, an archived
/// or installing fleet, and one whose setup needs review are each refused,
/// with no admission written.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleet_steer_refuses_self_other_workspace_and_inactive_targets() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let foreign = crew.foreign("other-workspace-fleet").await;
    let archived = crew.sibling_as("archived-fleet", ARCHIVED).await;
    let installing = crew.sibling_as("installing-fleet", INSTALLING).await;
    let stale = crew.sibling("stale-setup-fleet").await;
    crew.stale_setup(&stale).await;
    let (run, fence) = crew.speaking.run_seeded().await;
    let own = crew.run().fleet.clone();
    let own_rows = crew.admissions(&own).await.len();

    let cases = [
        (&own, 409, "UZ-AGT-018", Some("self")),
        (&foreign, 404, "UZ-AGT-009", None),
        (&archived, 409, "UZ-AGT-012", Some("archived")),
        (&installing, 409, "UZ-AGT-012", Some("installing")),
        (&stale, 409, "UZ-AUTHOR-009", None),
    ];
    for (target, status, code, state) in cases {
        let (answered, refused) = crew.steer((&run, fence), target, "scene the check").await;
        assert_eq!(
            (answered, &refused["error_code"]),
            (status, &json!(code)),
            "{target}: {refused}"
        );
        if let Some(state) = state {
            assert_eq!(refused["current_state"], state, "{refused}");
        }
    }
    assert_eq!(crew.admissions(&own).await.len(), own_rows);
    for target in [&foreign, &archived, &installing, &stale] {
        assert!(
            crew.admissions(target).await.is_empty(),
            "{target} was written"
        );
    }
    crew.finish(supervisor).await;
}

/// Onboarding's first-steer read stays false after a fleet's
/// steer, and the person-steer filter returns no row for it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleet_steer_is_not_a_person_steer_to_onboarding_and_filters() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let target = crew.sibling("steer-target").await;
    let (run, fence) = crew.speaking.run_seeded().await;
    let (status, answer) = crew.steer((&run, fence), &target, "scene the check").await;
    assert_eq!(status, 202, "{answer}");

    let scene = crew.run();
    let signals = Preferences::new(scene.booted.database.clone(), Entropy::new())
        .signals(&scene.workspace, &scene.tenant)
        .await
        .expect("the onboarding read");
    assert!(
        !signals.has_steer_event,
        "a fleet's steer is not a person's first steer"
    );
    let filter = Filter {
        actor_like: Some(prefix_to_like("steer:")),
        since: None,
    };
    let rows = History::new(scene.booted.database.clone())
        .page_for_workspace(&scene.workspace, Some(&target), &filter, None, 50)
        .await
        .expect("the filtered read");
    assert!(rows.is_empty(), "{rows:?}");
    crew.finish(supervisor).await;
}

/// A person who took the operation id a fleet's steer derives
/// keeps it: the steer is a conflict, and the one row is the person's.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleet_steer_refuses_a_key_a_person_took() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let target = crew.sibling("steer-target").await;
    let (run, fence) = crew.speaking.run_seeded().await;
    let text = "open the fix";
    let digest = af_core::etag::sha256_hex(text.as_bytes());
    let operation = format!("{}:{}", crew.run().event_id, &digest[..KEY_DIGEST_HEX]);

    let scene = crew.run();
    let admissions = Admissions::for_tests(scene.booted.database.clone(), scene.booted.bus.clone());
    let person = steer_actor(PERSON);
    Steer::new(admissions)
        .append(
            &target,
            &scene.workspace,
            &person,
            r#"{"message":"open the fix"}"#,
            Some(&operation),
        )
        .await
        .expect("the person's message is admitted");

    let (status, refused) = crew.steer((&run, fence), &target, text).await;
    assert_eq!(
        (status, &refused["error_code"], &refused["current_state"]),
        (409, &json!("UZ-AGT-016"), &json!("admitted")),
        "{refused}"
    );
    let rows = crew.admissions(&target).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].actor, person);
    crew.finish(supervisor).await;
}

/// A sender archived while its run is live steers nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleet_steer_refuses_a_sender_that_is_no_longer_installed() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let target = crew.sibling("steer-target").await;
    let (run, fence) = crew.speaking.run_seeded().await;
    let sender = crew.run().fleet.clone();
    crew.alter(
        "UPDATE fleet.fleets SET status = 'archived' WHERE id = $1",
        &sender,
    )
    .await;

    let (status, refused) = crew.steer((&run, fence), &target, "scene the check").await;
    assert_eq!(
        (status, &refused["error_code"]),
        (404, &json!("UZ-RUN-006")),
        "{refused}"
    );
    let rows = crew.admissions(&target).await;
    assert!(rows.is_empty(), "{rows:?}");
    crew.finish(supervisor).await;
}

/// The route says what is wrong with a body: a target that is not an id, an
/// empty text, a field it does not know and a body with no fence are each a
/// 400 with their own sentence, and nothing is written.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleet_steer_refuses_bad_bodies_with_the_route_sentences() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let target = crew.sibling("steer-target").await;
    let (run, fence) = crew.speaking.run_seeded().await;
    let path = format!("/v1/runners/me/runs/{run}/steers");
    let id = target.as_str();

    for (body, sentence) in [
        (
            json!({"fencing_token": fence, "fleet_id": "docs-bot", "text": "hi"}),
            "fleet_id must be a valid UUIDv7",
        ),
        (
            json!({"fencing_token": fence, "fleet_id": id, "text": ""}),
            "text must be 1 to 8192 bytes with no NUL",
        ),
        (
            json!({"fencing_token": fence, "fleet_id": id, "text": "a\u{0}b"}),
            "text must be 1 to 8192 bytes with no NUL",
        ),
        (
            json!({"fencing_token": fence, "fleet_id": id, "text": "hi", "extra": 1}),
            "Malformed steer body",
        ),
        (
            json!({"fleet_id": id, "text": "hi"}),
            "Malformed steer body",
        ),
    ] {
        let (status, refused) = crew.post(&path, &body).await;
        assert_eq!(status, 400, "{body}: {refused}");
        assert_eq!(refused["detail"], sentence, "{body}: {refused}");
    }
    let rows = crew.admissions(&target).await;
    assert!(rows.is_empty(), "{rows:?}");
    crew.finish(supervisor).await;
}
