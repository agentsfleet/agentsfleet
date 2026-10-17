//! The leased fleet's own tenant also authors: an owner, a priced model behind a
//! sealed key, a funded wallet, and queued turns the production admission
//! places on the same host as the runtime lease.

use super::fixture::Running;
use afd_authoring::{
    Access, Claimed, Drafts,
    types::{CreateDraft, Draft, Source},
};
use afd_core::{clock, id::Uuid7, money::NANOS_PER_USD};
use afd_crypto::{aad::Aad, entropy::Entropy, envelope::Sealer, secret::Kek};
use afd_db::{Db, test_util::mint_id};

/// Past any bound a turn reserves and any slice a renewal meters.
pub(super) const FUNDED: i64 = 1_000 * NANOS_PER_USD;
const SECRET_REF: &str = "authoring-model";
const PROVIDER: &str = "openai";
const KEK_HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const IDENTITY: &str =
    "SELECT tenant_id::text, workspace_id::text FROM core.fleets WHERE id = $1::uuid";
const PERSON: &str = "\
INSERT INTO core.users(id,tenant_id,oidc_subject,email,created_at,updated_at)
VALUES($1::uuid,$2::uuid,$1,$1,$3,$3)";
const MODEL: &str = "\
INSERT INTO core.model_library(id,model_id,provider,context_cap_tokens,input_nanos_per_mtok,
cached_input_nanos_per_mtok,output_nanos_per_mtok,created_at,updated_at,inference)
VALUES($1::uuid,$2,$3,128000,$4,$4,$4,$5,$5,$6)";
const ENTRY: &str = "\
INSERT INTO core.tenant_model_entries(id,tenant_id,model_id,secret_ref,created_at,updated_at)
VALUES($1::uuid,$2::uuid,$3,$4,$5,$5)";
const SECRET: &str = "\
INSERT INTO vault.secrets(id,workspace_id,key_name,encrypted_dek,dek_nonce,dek_tag,nonce,
ciphertext,tag,kek_version,meta_provider,meta_has_key,created_at,updated_at)
VALUES($1::uuid,$2::uuid,$3,$4,$5,$6,$7,$8,$9,$10,$11,true,$12,$12)";

pub(super) struct Seat {
    pub(super) tenant: String,
    workspace: Uuid7,
    owner: Uuid7,
    entry: Uuid7,
    catalogue: Uuid7,
}

impl Seat {
    pub(super) async fn create(running: &Running) -> Self {
        let database = &running.fixtures.database;
        let (tenant, workspace): (String, String) = sqlx::query_as(IDENTITY)
            .bind(&running.fleet)
            .fetch_one(&mut *database.acquire().await.expect("identity"))
            .await
            .expect("the leased fleet's tenant and workspace");
        let seat = Self {
            tenant,
            workspace: id(&workspace),
            owner: id(&mint_id()),
            entry: id(&mint_id()),
            catalogue: id(&mint_id()),
        };
        seat.seed(database).await;
        running
            .fixtures
            .seed_wallet(&seat.tenant, FUNDED, clock::now().as_millis())
            .await;
        seat
    }

    async fn seed(&self, database: &Db) {
        let now = clock::now().as_millis();
        let model = format!("gpt-4o-{}", self.catalogue);
        let mut connection = database.acquire().await.expect("seat seed");
        sqlx::query(PERSON)
            .bind(self.owner.as_str())
            .bind(&self.tenant)
            .bind(now)
            .execute(&mut *connection)
            .await
            .expect("the tenant's owner");
        sqlx::query(MODEL)
            .bind(self.catalogue.as_str())
            .bind(&model)
            .bind(PROVIDER)
            .bind(NANOS_PER_USD)
            .bind(now)
            .bind(sqlx::types::Json(afd_wire::inference::ModelInference {
                api: afd_wire::inference::InferenceApi::Responses,
                base_url: "https://models.example.com/v1".into(),
                dialect: None,
                max_output_tokens: 8192,
            }))
            .execute(&mut *connection)
            .await
            .expect("an isolated priced model");
        sqlx::query(ENTRY)
            .bind(self.entry.as_str())
            .bind(&self.tenant)
            .bind(&model)
            .bind(SECRET_REF)
            .bind(now)
            .execute(&mut *connection)
            .await
            .expect("the owner's registered model");
        let kek = Kek::from_hex(KEK_HEX).expect("fixture key");
        let sealed = Sealer::new()
            .seal(
                &kek,
                &Aad::new(self.workspace.as_str(), SECRET_REF),
                br#"{"provider":"openai","api_key":"fixture-authoring-key"}"#,
            )
            .expect("the fixture key seals");
        sqlx::query(SECRET)
            .bind(mint_id())
            .bind(self.workspace.as_str())
            .bind(SECRET_REF)
            .bind(sealed.wrapped_dek())
            .bind(sealed.dek_nonce().as_slice())
            .bind(sealed.dek_tag().as_slice())
            .bind(sealed.payload_nonce().as_slice())
            .bind(sealed.payload_ciphertext())
            .bind(sealed.payload_tag().as_slice())
            .bind(sealed.kek_version())
            .bind(PROVIDER)
            .bind(now)
            .execute(&mut *connection)
            .await
            .expect("the sealed owner key");
    }

    /// Drafts over `database`: the primary's pool, or a replica's.
    pub(super) fn drafts(database: &Db) -> Drafts {
        Drafts::new(database.clone(), Entropy::new())
    }

    /// Queues one turn through the production create path.
    pub(super) async fn queue(&self, drafts: &Drafts) -> Draft {
        let access = Access {
            workspace: &self.workspace,
            subject: self.owner.as_str(),
        };
        let request = CreateDraft {
            source: Source::Definition {
                definition: Box::new(definition()),
            },
            authoring_model_ref: self.entry.to_string(),
            message: Some("Investigate the checkout".to_owned()),
            operation_id: id(&mint_id()),
        };
        drafts
            .create(access, request, clock::now())
            .await
            .expect("queue a private turn")
            .draft
    }

    /// The production admission transaction, pointed at this seat's draft.
    pub(super) async fn admit(drafts: &Drafts, runner: &Uuid7, draft: &Draft) -> Option<Claimed> {
        drafts
            .claim_queued(runner, &draft.id)
            .await
            .expect("admission answers")
    }

    /// Removes the one global row the seat wrote; the lane reset owns the rest.
    pub(super) async fn remove(self, database: &Db) {
        sqlx::query("DELETE FROM core.model_library WHERE id = $1::uuid")
            .bind(self.catalogue.as_str())
            .execute(&mut *database.acquire().await.expect("seat cleanup"))
            .await
            .expect("remove the isolated catalogue row");
    }
}

fn definition() -> afd_fleet_runtime::Definition {
    serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "description": "Explain failures.",
        "instructions_md": "Investigate failures.\n",
        "config": {"name": "checkout-watch", "x-agentsfleet": {
            "triggers": [], "tools": [], "credentials": [], "network": {"allow": []},
            "gates": {"timeout_ms": 60000, "rules": []},
            "budget": {"daily_nanos": NANOS_PER_USD.to_string()}
        }}
    }))
    .expect("native setup passes the shared validator")
}

fn id(raw: &str) -> Uuid7 {
    Uuid7::parse(raw).expect("a UUIDv7")
}
