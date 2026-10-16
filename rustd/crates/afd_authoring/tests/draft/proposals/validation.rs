//! Invalid first-generation content makes no durable mutation.

use super::{ActivityEvent, Attempt, Fixture, Proposal, Source, json};
use afd_core::error_code::INVALID_REQUEST;
use afd_fleet_runtime::definition::MAX_INITIAL_INSTRUCTIONS_CHARS;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn invalid_initial_generation_has_no_candidate_or_activity_receipt() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let mut value = serde_json::to_value(crate::draft::definition()).expect("native value");
    *value.get_mut("instructions_md").expect("instructions") =
        json!("a".repeat(MAX_INITIAL_INSTRUCTIONS_CHARS + 1));
    let mut invalid = a.initialize(1);
    invalid.event = ActivityEvent::Proposal {
        proposal: Proposal::Initialize {
            definition: Box::new(serde_json::from_value(value).expect("valid while editing")),
        },
    };
    let error = f
        .drafts
        .activity(&f.runner, &a.lease, invalid)
        .await
        .expect_err("first generation bound");
    assert_eq!(error.code(), INVALID_REQUEST);
    assert_eq!(f.counts().await, (1, 0, 1));
    assert_eq!(a.activity_count(&f).await, 0);
    f.drafts
        .activity(&f.runner, &a.lease, a.initialize(1))
        .await
        .expect("unaccepted sequence remains available");
    assert_eq!(a.activity_count(&f).await, 1);
    f.cleanup().await;
}
