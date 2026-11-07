//! Context hints release a blocked acquisition at their publication deadline.

#![expect(
    clippy::expect_used,
    reason = "fixture setup and deadline regressions must fail the test"
)]

use super::super::ContextPublisher;
use crate::{Drafts, lease::guard::Counters};
use afd_core::{error_code, test_util::trace::Capture};
use afd_crypto::entropy::Entropy;
use afd_db::{config::DbRole, test_util::TestDatabase};
use afd_dragonfly::{
    Dragonfly, FleetStreams,
    config::{DragonflyConfig, DragonflyRole},
};
use afd_wire::{
    authoring::{Knowledge, Meter, Renew},
    context::ContextState,
};
use std::{sync::Arc, time::Duration};

#[tokio::test]
#[ignore = "requires live Postgres: make test-coverage-rustd"]
async fn a_blocked_context_hint_expires_and_releases_its_pool_waiter() {
    let (pool, drafts, streams) = setup().await;
    let id = Entropy::new()
        .uuid7(jiff::Timestamp::now())
        .expect("unique subject");
    let counters = counters();
    let held = pool.acquire().await.expect("occupy the only pool slot");
    let capture = Capture::install();
    let publisher = ContextPublisher::new(&streams, &drafts, &id, &id, &counters, id.clone(), 1);
    tokio::time::timeout(
        Duration::from_secs(4),
        publisher.send(ContextState::Summarizing),
    )
    .await
    .expect("publication is bounded before pool acquisition times out");
    assert_deadline(&capture, &id);
    drop(held);
    let available = tokio::time::timeout(Duration::from_secs(1), pool.acquire())
        .await
        .expect("cancelled publication did not retain its waiter")
        .expect("pool serves again");
    drop(available);
    pool.close().await;
}

async fn setup() -> (afd_db::Db, Drafts, FleetStreams) {
    let pool = TestDatabase::shared()
        .open(
            DbRole::Api,
            &[
                ("DATABASE_POOL_SIZE_API", "1"),
                ("DATABASE_MIN_POOL_SIZE_API", "1"),
            ],
        )
        .await;
    let drafts = Drafts::new(
        pool.clone(),
        Entropy::new(),
        Arc::new(afd_billing::ledger::LedgerBiller::on_system_clock(
            pool.clone(),
        )),
    );
    let queue = Dragonfly::unreachable(&DragonflyConfig::from_url(
        DragonflyRole::Default,
        "redis://127.0.0.1:1".into(),
    ))
    .expect("lazy queue handle");
    (pool, drafts, FleetStreams::new(queue))
}

fn assert_deadline(capture: &Capture, id: &afd_core::id::Uuid7) {
    let failure = capture.only("draft_preview_failed");
    assert_eq!(failure.field("reason"), Some("deadline"));
    assert_eq!(failure.field("frames"), Some("0"));
    assert_eq!(failure.field("dispatch_id"), Some(id.as_str()));
    assert_eq!(
        failure.field("error_code"),
        Some(error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
    assert_eq!(
        capture
            .events()
            .iter()
            .filter(|event| {
                matches!(
                    event.field("event"),
                    Some(
                        "draft_preview_started"
                            | "draft_preview_failed"
                            | "draft_preview_completed"
                    )
                )
            })
            .count(),
        2
    );
}

fn counters() -> Counters {
    let request = garde::Unvalidated::new(Renew {
        fence: "1".into(),
        usage: Meter {
            knowledge: Knowledge::Known,
            calls: 0,
            input_tokens: Some(0),
            output_tokens: Some(0),
            runtime_ms: 0,
            provider_cost_nanos: Some("0".into()),
        },
    })
    .validate()
    .expect("wire bounds");
    Counters::renew(&request).expect("positive lease fence")
}
