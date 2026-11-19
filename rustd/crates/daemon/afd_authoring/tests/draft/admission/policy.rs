//! An issued authoring attempt owns its accepted policy after a deployment changes.

use super::{claim, funded, queue};
use afd_authoring::turn_policy::{TurnPolicy, TurnSettings};

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn issue_carries_and_saves_the_checked_authoring_allowances() {
    let mut f = funded(1).await;
    let settings = garde::Unvalidated::new(TurnSettings {
        max_model_calls: 1,
        max_wall_seconds: 60,
        max_proposal_corrections: 0,
        provider_timeout_seconds: 30,
    })
    .validate()
    .expect("bounded accepted policy");
    f.drafts = f
        .drafts
        .clone()
        .with_turn_policy(TurnPolicy::new(&settings));
    let draft = queue(&f, "Explain the failed checkout").await;
    let lease = claim(&f, &draft).await.expect("funded attempt");
    assert_eq!(lease.limits.max_calls, 1);
    assert_eq!(lease.limits.wall_seconds, 60);
    assert_eq!(lease.limits.max_proposal_corrections, 0);
    assert_eq!(lease.limits.provider_timeout_seconds, 30);
    let row: (i32, i32, i32, i32) = sqlx::query_as("SELECT max_model_calls,max_wall_seconds,max_proposal_corrections,provider_timeout_seconds FROM authoring.attempts WHERE lease_id=$1")
        .bind(af_core::id::Uuid7::parse(&lease.lease_id).expect("issued lease identity"))
        .fetch_one(&mut *f.connection().await).await.expect("saved allowance snapshot");
    assert_eq!(row, (1, 60, 0, 30));
    f.cleanup().await;
}
