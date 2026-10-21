//! A provider may reject a credential after Stop or model revocation commits.

use super::{Fixture, Provider, call, claim, funded, queue, state, unknown_report};
use crate::draft::funnel::{FINISHED, only, property};
use afd_authoring::{Claimed, ProviderRequest, ProviderResponse, Transport};
use afd_core::{error_code, id::Uuid7};
use serde_json::json;
use std::sync::{Arc, atomic::AtomicUsize};
use tokio::{
    sync::Notify,
    time::{Duration, timeout},
};

const DEADLINE: Duration = Duration::from_secs(10);

#[derive(Debug)]
struct Blocked {
    entered: Notify,
    release: Notify,
    provider: Provider,
}

#[async_trait::async_trait]
impl Transport for Blocked {
    async fn send(&self, request: ProviderRequest) -> afd_authoring::Result<ProviderResponse> {
        self.entered.notify_one();
        self.release.notified().await;
        self.provider.send(request).await
    }
}

enum Interruption {
    Stop,
    RevokeModel,
}

async fn interrupt(f: &Fixture, draft: &Uuid7, interruption: &Interruption) {
    match interruption {
        Interruption::Stop => {
            f.drafts
                .cancel(f.access(), draft, 1)
                .await
                .expect("Stop commits");
        }
        Interruption::RevokeModel => {
            sqlx::query("DELETE FROM model.tenant_models WHERE id=$1::uuid")
                .bind(f.model.as_str())
                .execute(&mut *f.connection().await)
                .await
                .expect("revoke the owned model");
        }
    }
}

async fn exercise(interruption: Interruption) {
    let f = funded(2).await;
    let draft = queue(&f, "Credential is rejected after the turn loses authority").await;
    let lease = claim(&f, &draft).await.expect("admitted");
    let provider = Arc::new(Blocked {
        entered: Notify::new(),
        release: Notify::new(),
        provider: Provider {
            count: AtomicUsize::new(0),
            status: 401,
            body: json!({"error":{"code":401,"message":"Invalid credentials"}}),
        },
    });
    let reply = Box::pin(call(&f, &lease, Arc::clone(&provider)));
    let interrupted = async {
        provider.entered.notified().await;
        interrupt(&f, &draft.id, &interruption).await;
        provider.release.notify_one();
    };
    let (reply, ()) = timeout(DEADLINE, async { tokio::join!(reply, interrupted) })
        .await
        .expect("the controlled provider race finishes");
    let expected = match interruption {
        Interruption::Stop => (error_code::AUTHORING_FENCE, "cancelled"),
        Interruption::RevokeModel => (error_code::AUTHORING_MODEL, "failed"),
    };
    assert_eq!(
        reply.expect_err("late private reply refused").code(),
        expected.0
    );
    settle(&f, &lease).await;
    let finished = only(f.reported(FINISHED), "one real terminal message transition");
    assert_eq!(property(&finished, "state").as_deref(), Some(expected.1));
    f.cleanup().await;
}

async fn settle(f: &Fixture, lease: &Claimed) {
    let id = Uuid7::parse(&lease.lease_id).expect("lease");
    let report = unknown_report(lease, 1);
    let first = f
        .drafts
        .report(&f.runner, &id, report.clone())
        .await
        .expect("initial late accounting");
    assert!(first.settled);
    assert!(!first.replayed);
    let duplicate = f
        .drafts
        .report(&f.runner, &id, report)
        .await
        .expect("replayed late accounting");
    assert!(duplicate.settled);
    assert!(duplicate.replayed);
    assert_eq!(state(f, lease).await, ("settled".into(), false, 1, false));
}

// Value: protects=Stop owns the terminal event even when a late provider rejects;
// fails_when=refusal evidence emits a finish without updating a running message.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn stopped_authentication_reply_finishes_only_once() {
    exercise(Interruption::Stop).await;
}

// Value: protects=model revocation and key rejection share one terminal transition;
// fails_when=authorization refusal and provider refusal both owe a finished event.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn revoked_model_authentication_reply_finishes_only_once() {
    exercise(Interruption::RevokeModel).await;
}
