use super::*;

/// The isolation a setup run's runner is enrolled with: none, as setup runs
/// build no sandbox.
const POLICY: afd_db::test_util::RunnerPolicy<'static> = afd_db::test_util::RunnerPolicy {
    sandbox_tier: "dev_none",
    network_policy: "deny_all_egress",
};

pub(super) struct Setup {
    pub(super) runs: [Uuid7; 3],
    pub(super) settlements: [Uuid7; 2],
    pub(super) ledger_rows: [Uuid7; 2],
    workspace: Uuid7,
    draft: Uuid7,
    creator: Uuid7,
    runner: Uuid7,
}

impl Setup {
    pub(super) async fn seed(owner: &Fixture) -> Self {
        let setup = Self {
            runs: [mint_id(), mint_id(), mint_id()],
            settlements: [mint_id(), mint_id()],
            ledger_rows: owner.ledger_rows.clone(),
            workspace: mint_id(),
            draft: mint_id(),
            creator: mint_id(),
            runner: mint_id(),
        };
        let mut connection = owner
            .migrator
            .acquire()
            .await
            .expect("setup seed connection");
        sqlx::query(
            "INSERT INTO identity.workspaces(id,tenant_id,created_at) VALUES($1,$2,to_timestamp(1))",
        )
        .bind(&setup.workspace)
        .bind(&owner.tenant)
        .execute(&mut *connection)
        .await
        .expect("workspace");
        afd_db::test_util::RunnerSeed::new(&setup.runner, "active", POLICY)
            .insert(&mut *connection)
            .await
            .expect("the runner the setup runs ran on");
        for run in &setup.runs {
            // A setup run its draft owns: a settled run is history that outlives the draft.
            sqlx::query("INSERT INTO thread.runs(id,runner_id,draft_id,workspace_id,tenant_id,turn_id,turn_created_at,posture,provider,model,last_metered_at,fencing_token,run_expires_at,status,posted_message_count,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$1,to_timestamp(101),'platform','setup','setup-model',to_timestamp(101),1,to_timestamp(101),$6,0,to_timestamp(101),to_timestamp(101))")
                .bind(run).bind(&setup.runner).bind(&setup.draft).bind(&setup.workspace).bind(&owner.tenant)
                .bind(afd_runner::sql::RUN_STATUS_REPORTED)
                .execute(&mut *connection).await.expect("a reported setup run without retained private draft");
        }
        for ((settlement, run), charged) in setup
            .settlements
            .iter()
            .zip(&setup.runs)
            .zip([17_i64, 19_i64])
        {
            // A settled setup run's charge is its usage line, as the report logs it.
            sqlx::query("INSERT INTO billing.usage_lines(id,work_type,work_id,seq, kind,tenant_id,workspace_id,run_id,provider,model,posture,requested_at,input_token_count,cached_input_token_count,cache_write_token_count,cache_write_5m_token_count,cache_write_1h_token_count,output_token_count,runtime_ms,unpriced_token_count,charged_nanos,settled_at,created_at,updated_at) VALUES($1,$5,$2,0, 'model',$4,$6,$2,'setup','setup-model','platform',to_timestamp(101),23,0,0,0,0,29,31,0,$3,to_timestamp(101),to_timestamp(101),to_timestamp(101))")
                .bind(settlement).bind(run).bind(charged)
                .bind(&owner.tenant).bind(afd_billing::sql::work::AUTHORING_ATTEMPT).bind(&setup.workspace)
                .execute(&mut *connection).await.expect("a settled setup line, including a paid failed run");
        }
        sqlx::query(
            "UPDATE billing.usage_lines SET created_at=to_timestamp(101) WHERE tenant_id=$1",
        )
        .bind(&owner.tenant)
        .execute(&mut *connection)
        .await
        .expect("tied timestamps across charge sources");
        setup
    }

    pub(super) async fn link(&self, owner: &Fixture) -> Uuid7 {
        let fleet = mint_id();
        let mut connection = owner
            .migrator
            .acquire()
            .await
            .expect("link seed connection");
        sqlx::query("INSERT INTO identity.users(id,tenant_id,oidc_subject,email,created_at,updated_at) VALUES($1,$2,$1,'fixture@example.invalid',now(),now())")
            .bind(&self.creator).bind(&owner.tenant).execute(&mut *connection).await.expect("private chat creator");
        sqlx::query("INSERT INTO fleet.fleets(id,workspace_id,tenant_id,name,status,required_tags,memory_reads_workspace,memory_publishes_workspace,resource_version,created_at,updated_at) VALUES($1,$2,$3,'saved-setup','active','{}',false,false,1,now(),now())")
            .bind(&fleet).bind(&self.workspace).bind(&owner.tenant).execute(&mut *connection).await.expect("public saved fleet");
        // Every turn names its thread, so a fleet's thread is written with it, as creation writes it.
        sqlx::query("INSERT INTO thread.threads(id,fleet_id,fencing_token,run_expires_at,created_at,updated_at) VALUES($1,$1,0,clock_timestamp()-interval '1 millisecond',now(),now())")
            .bind(&fleet).execute(&mut *connection).await.expect("public saved fleet's thread");
        sqlx::query("INSERT INTO setup.drafts(id,workspace_id,tenant_id,user_id,title,status,source_type,fleet_id,last_message_sequence,resource_version,created_at,updated_at) VALUES($1,$2,$3,$4,'private title','closed','blank',$5,0,1,now(),now())")
            .bind(&self.draft).bind(&self.workspace).bind(&owner.tenant).bind(&self.creator).bind(&fleet)
            .execute(&mut *connection).await.expect("private draft linked to saved fleet");
        // A draft's thread is written with it too, as draft creation writes it.
        sqlx::query("INSERT INTO thread.threads(id,draft_id,fencing_token,run_expires_at,created_at,updated_at) VALUES($1,$1,0,clock_timestamp()-interval '1 millisecond',now(),now())")
            .bind(&self.draft).execute(&mut *connection).await.expect("private draft's thread");
        fleet
    }

    pub(super) async fn remove_private_chat(&self, owner: &Fixture) {
        sqlx::query("DELETE FROM setup.drafts WHERE id=$1")
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

    /// Removes the runner the setup runs ran on; its runs go with it.
    pub(super) async fn remove(&self, owner: &Fixture) {
        sqlx::query("DELETE FROM runner.runners WHERE id=$1")
            .bind(&self.runner)
            .execute(
                &mut *owner
                    .migrator
                    .acquire()
                    .await
                    .expect("runner removal connection"),
            )
            .await
            .expect("the setup runs' runner removal");
    }
}
