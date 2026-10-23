//! An isolated admitted row exercises machine guards without enabling claim early.

use super::{
    fixture::{Fixture, now},
    mint,
};
use afd_authoring::{
    Access, ActivityInput,
    types::{Draft, Source},
};
use afd_core::id::Uuid7;
use afd_db::test_util::{RunnerPolicy, RunnerSeed};
use afd_wire::authoring::{ActivityEvent, ActivityRequest, Proposal};

const ATTEMPT: &str = "\
INSERT INTO authoring.attempts(id,lease_id,runner_id,workspace_id,tenant_id,user_id,
draft_id,message_sequence,fencing_token,model_token_hash,model_snapshot,status,
lease_expires_at,expires_at,reserved_nanos,usage_unknown,call_count,output_token_count,
reported_runtime_ms,last_activity_sequence,created_at,updated_at)
VALUES($1::uuid,$2::uuid,$3::uuid,$4::uuid,$5::uuid,$6::uuid,$7::uuid,1,1,$1,$8::jsonb,
'active',now()+interval '1 minute',now()+interval '1 minute',0,false,0,0,0,0,now(),now())";

/// The host's isolation: no sandbox and no egress.
const POLICY: RunnerPolicy<'static> = RunnerPolicy {
    sandbox_tier: "dev_none",
    network_policy: "deny_all_egress",
};

pub(super) struct Attempt {
    pub(super) id: Uuid7,
    pub(super) lease: Uuid7,
    pub(super) draft: Draft,
    sequence: String,
}

impl Attempt {
    pub(super) async fn start(f: &Fixture, access: Access<'_>, source: Source) -> Self {
        RunnerSeed::new(f.runner.as_str(), "active", POLICY)
            .insert(&mut *f.connection().await)
            .await
            .expect("isolated authoring runner");
        Self::admit(f, access, source).await
    }

    /// Another native draft using the fixture's already registered runner.
    pub(super) async fn admit(f: &Fixture, access: Access<'_>, source: Source) -> Self {
        let mut request = f.request(mint(), Some("Investigate"));
        request.source = source;
        let created = f
            .drafts
            .create(access, request, now())
            .await
            .expect("queue native chat");
        let attempt = Self {
            id: mint(),
            lease: mint(),
            draft: created.draft,
            sequence: created.accepted.expect("first message accepted").sequence,
        };
        let mut connection = f.connection().await;
        let model = afd_credential::provider::registered_model(
            &mut connection,
            &f.tenant,
            &f.model.to_string(),
        )
        .await
        .expect("registered fixture model")
        .expect("owner model exists");
        let creator = if access.subject == f.owner.as_str() {
            &f.owner
        } else {
            &f.member
        };
        sqlx::query(ATTEMPT)
            .bind(attempt.id.as_str())
            .bind(attempt.lease.as_str())
            .bind(f.runner.as_str())
            .bind(f.workspace.as_str())
            .bind(f.tenant.as_str())
            .bind(creator.as_str())
            .bind(attempt.draft.id.as_str())
            .bind(serde_json::to_string(&model).expect("pinned nonsecret metadata"))
            .execute(&mut *connection)
            .await
            .expect("admitted attempt row");
        sqlx::query("UPDATE authoring.messages SET status='running' WHERE draft_id=$1::uuid")
            .bind(attempt.draft.id.as_str())
            .execute(&mut *connection)
            .await
            .expect("owned running message");
        attempt
    }

    pub(super) fn initialize(&self, activity: u32) -> ActivityInput {
        ActivityRequest {
            fence: "1".into(),
            sequence: self.sequence.clone(),
            activity_sequence: activity.to_string(),
            event: ActivityEvent::Proposal {
                proposal: Proposal::Initialize {
                    definition: Box::new(super::definition()),
                },
            },
        }
    }

    pub(super) async fn activity_count(&self, f: &Fixture) -> i64 {
        sqlx::query_scalar(
            "SELECT count(*) FROM authoring.activity_receipts WHERE attempt_id=$1::uuid",
        )
        .bind(self.id.as_str())
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("owned activity count")
    }
}
