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
use afd_wire::authoring::{ActivityEvent, ActivityRequest, Proposal};

const RUNNER: &str = "\
INSERT INTO fleet.runners(id,host_id,token_hash,sandbox_tier,admin_state,labels,
network_policy,registry_allowlist,worker_count,degraded,last_seen_at,created_at,updated_at)
VALUES($1::uuid,$1,$1,'dev_none','active','[]','deny_all_egress','[]',1,false,$2,$2,$2)";
const ATTEMPT: &str = "\
INSERT INTO fleet.authoring_attempts(id,lease_id,runner_id,workspace_id,tenant_id,creator_id,
draft_id,message_sequence,fence,deletion_generation,capability_hash,model_snapshot,status,
lease_expires_at,deadline,reserved_nanos,settled_nanos,usage_unknown,calls,input_tokens,
output_tokens,activity_sequence,created_at,updated_at)
VALUES($1::uuid,$2::uuid,$3::uuid,$4::uuid,$5::uuid,$6::uuid,$7::uuid,1,1,0,$1,$8::jsonb,
'active',$9,$9,0,0,false,0,0,0,0,$10,$10)";

pub(super) struct Attempt {
    pub(super) id: Uuid7,
    pub(super) lease: Uuid7,
    pub(super) draft: Draft,
    sequence: String,
}

impl Attempt {
    pub(super) async fn start(f: &Fixture, access: Access<'_>, source: Source) -> Self {
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
        sqlx::query(RUNNER)
            .bind(f.runner.as_str())
            .bind(now().as_millis())
            .execute(&mut *connection)
            .await
            .expect("isolated authoring runner");
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
            .bind(now().as_millis() + 60_000)
            .bind(now().as_millis())
            .execute(&mut *connection)
            .await
            .expect("admitted attempt row");
        sqlx::query("UPDATE core.authoring_messages SET state='running' WHERE draft_id=$1::uuid")
            .bind(attempt.draft.id.as_str())
            .execute(&mut *connection)
            .await
            .expect("owned running message");
        attempt
    }

    pub(super) fn initialize(&self, activity: u32) -> ActivityInput {
        ActivityRequest {
            fence: "1".into(),
            deletion_generation: "0".into(),
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
            "SELECT count(*) FROM fleet.authoring_activity_receipts WHERE attempt_id=$1::uuid",
        )
        .bind(self.id.as_str())
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("owned activity count")
    }
}
