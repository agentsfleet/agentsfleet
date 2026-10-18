use super::*;

pub(super) struct Setup {
    pub(super) runner: String,
    pub(super) attempts: [String; 3],
    pub(super) settlements: [String; 2],
    pub(super) ledger_rows: [String; 2],
    workspace: String,
    draft: String,
    creator: String,
}

impl Setup {
    pub(super) async fn seed(owner: &Fixture) -> Self {
        let setup = Self {
            runner: mint_id(),
            attempts: [mint_id(), mint_id(), mint_id()],
            settlements: [mint_id(), mint_id()],
            ledger_rows: owner.ledger_rows.clone(),
            workspace: mint_id(),
            draft: mint_id(),
            creator: mint_id(),
        };
        let mut connection = owner
            .database
            .acquire()
            .await
            .expect("setup seed connection");
        sqlx::query(
            "INSERT INTO core.workspaces(id,tenant_id,created_at) VALUES($1::uuid,$2::uuid,1)",
        )
        .bind(&setup.workspace)
        .bind(&owner.tenant)
        .execute(&mut *connection)
        .await
        .expect("workspace");
        sqlx::query("INSERT INTO fleet.runners(id,host_id,token_hash,sandbox_tier,admin_state,labels,network_policy,registry_allowlist,worker_count,degraded,last_seen_at,created_at,updated_at) VALUES($1::uuid,$1,$1,'dev_none','active','[]','deny_all_egress','[]',1,false,1,1,1)")
            .bind(&setup.runner).execute(&mut *connection).await.expect("runner");
        for (index, attempt) in setup.attempts.iter().enumerate() {
            sqlx::query("INSERT INTO fleet.authoring_attempts(id,lease_id,runner_id,workspace_id,tenant_id,creator_id,draft_id,message_sequence,fence,deletion_generation,capability_hash,model_snapshot,status,lease_expires_at,deadline,reserved_nanos,settled_nanos,usage_unknown,calls,input_tokens,output_tokens,activity_sequence,created_at,updated_at) VALUES($1::uuid,$1::uuid,$2::uuid,$3::uuid,$4::uuid,$5::uuid,$6::uuid,$7,1,0,$1,'{\"model\":\"setup-model\",\"platform_managed\":true}','awaiting_accounting',101,101,999,0,true,0,0,0,0,101,101)")
                .bind(attempt).bind(&setup.runner).bind(&setup.workspace).bind(&owner.tenant).bind(&setup.creator).bind(&setup.draft)
                .bind(i64::try_from(index).expect("fixture index")).execute(&mut *connection).await.expect("attempt without retained private draft");
        }
        for ((settlement, attempt), (outcome, charged)) in setup
            .settlements
            .iter()
            .zip(&setup.attempts)
            .zip([("completed", 17_i64), ("failed", 19_i64)])
        {
            sqlx::query("INSERT INTO billing.authoring_settlements(id,attempt_id,report_id,accounting_digest,outcome,usage,charged_nanos,created_at) VALUES($1::uuid,$2::uuid,$1::uuid,$1,$3,'{\"knowledge\":\"known\",\"input_tokens\":23,\"output_tokens\":29,\"runtime_ms\":31}', $4,101)")
                .bind(settlement).bind(attempt).bind(outcome)
                .bind(charged).execute(&mut *connection).await.expect("settlement including paid failed attempt");
            sqlx::query("UPDATE fleet.authoring_attempts SET status='settled',usage_unknown=false,settled_nanos=$2 WHERE id=$1::uuid")
                .bind(attempt).bind(charged)
                .execute(&mut *connection).await.expect("settled obligation");
        }
        sqlx::query("UPDATE billing.usage_ledger SET created_at=101 WHERE tenant_id=$1::uuid")
            .bind(&owner.tenant)
            .execute(&mut *connection)
            .await
            .expect("tied timestamps across charge sources");
        setup
    }

    pub(super) async fn link(&self, owner: &Fixture) -> String {
        let fleet = mint_id();
        let mut connection = owner
            .database
            .acquire()
            .await
            .expect("link seed connection");
        sqlx::query("INSERT INTO core.users(id,tenant_id,oidc_subject,email,created_at,updated_at) VALUES($1::uuid,$2::uuid,$1,'fixture@example.invalid',1,1)")
            .bind(&self.creator).bind(&owner.tenant).execute(&mut *connection).await.expect("private chat creator");
        sqlx::query("INSERT INTO core.fleets(id,workspace_id,tenant_id,name,config_json,status,created_at,updated_at) VALUES($1::uuid,$2::uuid,$3::uuid,'saved-setup','{\"instructions\":\"private instructions\"}','active',1,1)")
            .bind(&fleet).bind(&self.workspace).bind(&owner.tenant).execute(&mut *connection).await.expect("public saved fleet");
        sqlx::query("INSERT INTO core.authoring_drafts(id,workspace_id,tenant_id,creator_id,operation_id,creation_digest,title,state,authoring_model_ref,candidate_revision,resource_version,last_sequence,cancelled_through,deletion_generation,linked_fleet_id,created_at,updated_at) VALUES($1::uuid,$2::uuid,$3::uuid,$4::uuid,$1::uuid,$1,'private title','closed','private model reference',0,1,0,0,0,$5::uuid,1,1)")
            .bind(&self.draft).bind(&self.workspace).bind(&owner.tenant).bind(&self.creator).bind(&fleet)
            .execute(&mut *connection).await.expect("private draft linked to saved fleet");
        fleet
    }

    pub(super) async fn remove_private_chat(&self, owner: &Fixture) {
        sqlx::query("DELETE FROM core.authoring_drafts WHERE id=$1::uuid")
            .bind(&self.draft)
            .execute(
                &mut *owner
                    .database
                    .acquire()
                    .await
                    .expect("private deletion connection"),
            )
            .await
            .expect("private text removal");
    }
}
