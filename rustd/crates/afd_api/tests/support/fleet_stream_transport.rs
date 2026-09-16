//! Real transport budgets and disconnect recovery, without a pub/sub replay promise.

#[path = "fleet_stream_transport_fixture.rs"]
mod fixture;

use std::time::Instant;

use afd_wire::report::Outcome;
use futures_util::future::join_all;
use serde_json::json;

use fixture::{Watched, assert_frame, chunk, completion, next_frame};

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_missing_fleet_refuses_without_retaining_a_stream_slot() {
    use super::{Fixture, SUBJECT};
    use crate::harness::{self, Fleet};
    use afd_auth::scope::{Scope, ScopeSet};
    use http::{Method, StatusCode};

    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .carrying_at_most(1)
    .router();
    let missing = afd_db::test_util::mint_id();
    for _ in 0..3 {
        let path = format!(
            "/v1/workspaces/{}/fleets/{missing}/events/stream",
            fixture.workspace
        );
        let response = harness::send(&router, Method::GET, &path, Some(&fixture.token), "").await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let document = harness::json_body(response).await;
        assert_eq!(document.get("detail"), Some(&json!("Fleet not found")));
        assert_eq!(
            document.get("error_code"),
            Some(&json!(afd_core::error_code::AGENTSFLEET_NOT_FOUND.as_str()))
        );
    }
    let path = format!(
        "/v1/workspaces/{}/fleets/{}/events/stream",
        fixture.workspace, fixture.fleet
    );
    let response = harness::send(&router, Method::GET, &path, Some(&fixture.token), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    drop(response);
    drop(router);
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn one_hundred_live_responses_share_one_subscription_and_release_every_reader() {
    let watched = Watched::create().await;
    for viewers in [1, 10, 100] {
        let mut bodies = join_all((0..viewers).map(|_| watched.open())).await;
        assert_eq!(watched.hub.readers(&watched.channel()), viewers);
        watched.ready(&mut bodies).await;
        assert_eq!(watched.hub.connections_opened(), 1);
        // Owning the only pool slot is a deterministic barrier: delivery below
        // cannot depend on another Postgres acquisition, at any viewer count.
        let held = watched
            .database()
            .acquire()
            .await
            .expect("streams return the pool slot");
        assert_eq!(watched.database().size(), 1);
        let started = Instant::now();
        let payload = chunk("fanout", "one");
        assert_eq!(watched.publish(&payload).await, 1);
        let frames = join_all(bodies.iter_mut().map(next_frame)).await;
        for frame in frames {
            assert_frame(&frame, 1, &payload);
        }
        let delivery_us = started.elapsed().as_micros();
        drop(held);
        drop(bodies);
        assert_eq!(watched.hub.readers(&watched.channel()), 0);
        watched.unsubscribed().await;
        eprintln!(
            "stream_transport_evidence={}",
            json!({
                "viewers": viewers, "redis_connections_opened": watched.hub.connections_opened(),
                "redis_subscribers": 1, "readers_after_drop": 0,
                "postgres_pool_size": watched.database().size(),
                "postgres_available_slots_during_delivery": 0,
                "database_query_count": null,
                "database_evidence": "delivery succeeds while the entire one-slot pool is held",
                "publish_to_all_viewers_us": delivery_us,
            })
        );
    }
    watched.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn one_hundred_quiet_viewers_receive_liveness_without_queries_or_sequence_gaps() {
    const VIEWERS: usize = 100;
    let watched = Watched::create().await;
    let mut bodies = join_all((0..VIEWERS).map(|_| watched.open())).await;
    watched.ready(&mut bodies).await;
    let held = watched
        .database()
        .acquire()
        .await
        .expect("only pool slot held");
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(15)).await;
    tokio::time::resume();
    let heartbeats = join_all(bodies.iter_mut().map(next_frame)).await;
    for heartbeat in heartbeats {
        assert_eq!(
            heartbeat,
            "event: heartbeat\ndata: {\"kind\":\"heartbeat\"}\n\n"
        );
    }
    assert_eq!(watched.hub.connections_opened(), 1);
    assert_eq!(watched.hub.readers(&watched.channel()), VIEWERS);
    let payload = chunk("after-heartbeat", "one");
    assert_eq!(watched.publish(&payload).await, 1);
    for frame in join_all(bodies.iter_mut().map(next_frame)).await {
        assert_frame(&frame, 1, &payload);
    }
    drop(held);
    drop(bodies);
    assert_eq!(watched.hub.readers(&watched.channel()), 0);
    watched.unsubscribed().await;
    watched.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn reconnect_recovers_a_missed_completion_from_durable_history() {
    let watched = Watched::create().await;
    let mut first = [watched.open().await];
    watched.ready(&mut first).await;
    drop(first);
    watched.unsubscribed().await;
    let event_id = watched.commit_without_publish().await;
    let payload = completion(&event_id);
    assert_eq!(
        watched.publish(&payload).await,
        0,
        "the absent viewer really misses the frame"
    );

    let started = Instant::now();
    let mut second = [watched.open().await];
    watched.ready(&mut second).await;
    let history = watched.history().await;
    let events = history
        .get("items")
        .and_then(serde_json::Value::as_array)
        .expect("history carries durable events");
    assert_eq!(
        events.len(),
        1,
        "one committed row is recovered by one bounded request"
    );
    let recovered = events.first().expect("the missed event exists");
    assert_eq!(recovered.get("event_id"), Some(&json!(event_id)));
    let expected_status = json!(Outcome::Processed.as_str());
    assert_eq!(recovered.get("status"), Some(&expected_status));
    let recovery_us = started.elapsed().as_micros();
    assert_eq!(watched.publish(&payload).await, 1);
    assert_frame(
        &next_frame(second.first_mut().expect("reconnected viewer")).await,
        1,
        &payload,
    );
    drop(second);
    assert_eq!(watched.hub.readers(&watched.channel()), 0);
    watched.unsubscribed().await;
    eprintln!(
        "stream_recovery_evidence={}",
        json!({
            "missed_publications": 1, "history_http_requests": 1, "recovered_rows": events.len(),
            "reconnect_and_history_us": recovery_us, "readers_after_drop": 0,
        })
    );
    watched.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn repeated_publications_remain_distinct_transport_frames_without_writing_history() {
    let watched = Watched::create().await;
    let mut bodies = [watched.open().await];
    watched.ready(&mut bodies).await;
    let payload = completion("duplicate");
    for sequence in [1, 2] {
        assert_eq!(watched.publish(&payload).await, 1);
        assert_frame(
            &next_frame(bodies.first_mut().expect("viewer")).await,
            sequence,
            &payload,
        );
    }
    let history = watched.history().await;
    assert_eq!(
        history.get("items"),
        Some(&json!([])),
        "pub/sub cannot create audit rows"
    );
    drop(bodies);
    watched.unsubscribed().await;
    watched.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn hub_shutdown_closes_the_response_instead_of_leaving_an_idle_stream() {
    use futures_util::StreamExt as _;
    let watched = Watched::create().await;
    let mut bodies = [watched.open().await];
    watched.ready(&mut bodies).await;
    watched.hub.shutdown();
    let body = bodies.first_mut().expect("viewer");
    let ended = tokio::time::timeout(fixture::DELIVERY_BUDGET, body.next())
        .await
        .expect("hub shutdown promptly ends the response");
    assert!(
        ended.is_none(),
        "the closed hub must not leave a silently idle response"
    );
    drop(bodies);
    watched.cleanup().await;
}
