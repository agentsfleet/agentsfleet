use super::*;

pub(super) struct Setup {
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
            attempts: [mint_id(), mint_id(), mint_id()],
            settlements: [mint_id(), mint_id()],
            ledger_rows: owner.ledger_rows.clone(),
            workspace: mint_id(),
            draft: mint_id(),
            creator: mint_id(),
        };
        let mut connection = owner
            .migrator
            .acquire()
            .await
            .expect("setup seed connection");
        sqlx::query(
            "INSERT INTO identity.workspaces(id,tenant_id,created_at) VALUES($1::uuid,$2::uuid,to_timestamp(1))",
        )
        .bind(&setup.workspace)
        .bind(&owner.tenant)
        .execute(&mut *connection)
        .await
        .expect("workspace");
        for (index, attempt) in setup.attempts.iter().enumerate() {
            // No runner: an attempt keeps its row after its runner goes (SET NULL).
            sqlx::query("INSERT INTO authoring.attempts(id,lease_id,runner_id,workspace_id,tenant_id,user_id,draft_id,message_sequence,fencing_token,model_token_hash,model_snapshot,status,lease_expires_at,expires_at,reserved_nanos,usage_unknown,call_count,output_token_count,reported_runtime_ms,last_activity_sequence,created_at,updated_at) VALUES($1::uuid,$1::uuid,NULL,$2::uuid,$3::uuid,$4::uuid,$5::uuid,$6,1,$1,'{\"model\":\"setup-model\",\"platform_managed\":true}','awaiting_accounting',to_timestamp(101),to_timestamp(101),999,true,0,0,31,0,to_timestamp(101),to_timestamp(101))")
                .bind(attempt).bind(&setup.workspace).bind(&owner.tenant).bind(&setup.creator).bind(&setup.draft)
                .bind(i64::try_from(index).expect("fixture index")).execute(&mut *connection).await.expect("attempt without retained private draft");
        }
        for ((settlement, attempt), (outcome, charged)) in setup
            .settlements
            .iter()
            .zip(&setup.attempts)
            .zip([("completed", 17_i64), ("failed", 19_i64)])
        {
            // A settled setup chat's charge is its usage line, as the report logs it.
            sqlx::query("INSERT INTO billing.usage_lines(id,work_type,work_id,seq,tenant_id,workspace_id,authoring_attempt_id,provider,model,posture,requested_at,input_token_count,cached_input_token_count,cache_write_token_count,cache_write_5m_token_count,cache_write_1h_token_count,output_token_count,runtime_ms,unpriced_token_count,charged_nanos,settled_at,created_at,updated_at) VALUES($1::uuid,'authoring_attempt',$2::uuid,0,$4::uuid,$5::uuid,$2::uuid,'setup','setup-model','platform',to_timestamp(101),23,0,0,0,0,29,31,0,$3,to_timestamp(101),to_timestamp(101),to_timestamp(101))")
                .bind(settlement).bind(attempt).bind(charged)
                .bind(&owner.tenant).bind(&setup.workspace)
                .execute(&mut *connection).await.expect("a settled setup line, including a paid failed attempt");
            sqlx::query("UPDATE authoring.attempts SET status='settled',usage_unknown=false,outcome=$2 WHERE id=$1::uuid")
                .bind(attempt).bind(outcome)
                .execute(&mut *connection).await.expect("settled obligation");
        }
        sqlx::query(
            "UPDATE billing.usage_lines SET created_at=to_timestamp(101) WHERE tenant_id=$1::uuid",
        )
        .bind(&owner.tenant)
        .execute(&mut *connection)
        .await
        .expect("tied timestamps across charge sources");
        setup
    }

    pub(super) async fn link(&self, owner: &Fixture) -> String {
        let fleet = mint_id();
        let mut connection = owner
            .migrator
            .acquire()
            .await
            .expect("link seed connection");
        sqlx::query("INSERT INTO identity.users(id,tenant_id,oidc_subject,email,created_at,updated_at) VALUES($1::uuid,$2::uuid,$1,'fixture@example.invalid',now(),now())")
            .bind(&self.creator).bind(&owner.tenant).execute(&mut *connection).await.expect("private chat creator");
        sqlx::query("INSERT INTO fleet.fleets(id,workspace_id,tenant_id,name,status,required_tags,memory_reads_workspace,memory_publishes_workspace,resource_version,created_at,updated_at) VALUES($1::uuid,$2::uuid,$3::uuid,'saved-setup','active','{}',false,false,1,now(),now())")
            .bind(&fleet).bind(&self.workspace).bind(&owner.tenant).execute(&mut *connection).await.expect("public saved fleet");
        sqlx::query("INSERT INTO authoring.drafts(id,workspace_id,tenant_id,user_id,title,status,source_type,fleet_id,last_message_sequence,cancelled_through_sequence,resource_version,created_at,updated_at) VALUES($1::uuid,$2::uuid,$3::uuid,$4::uuid,'private title','closed','blank',$5::uuid,0,0,1,now(),now())")
            .bind(&self.draft).bind(&self.workspace).bind(&owner.tenant).bind(&self.creator).bind(&fleet)
            .execute(&mut *connection).await.expect("private draft linked to saved fleet");
        fleet
    }

    pub(super) async fn remove_private_chat(&self, owner: &Fixture) {
        sqlx::query("DELETE FROM authoring.drafts WHERE id=$1::uuid")
            .bind(&self.draft)
            .execute(
                &mut *owner
                    .migrator
                    .acquire()
                    .await
                    .expect("private deletion connection"),
            )
            .await
            .expect("private text removal");
    }
}
