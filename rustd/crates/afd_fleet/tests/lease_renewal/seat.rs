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
use afd_db::{
    Db,
    test_util::{ModelSeed, mint_id},
};

/// The seated model routes through the Responses API.
const RESPONSES_INFERENCE: &str =
    r#"{"api":"responses","base_url":"https://models.example.com/v1","max_output_tokens":8192}"#;

/// Past any bound a turn reserves and any slice a renewal meters.
pub(super) const FUNDED: i64 = 1_000 * NANOS_PER_USD;
const SECRET_REF: &str = "authoring-model";
const PROVIDER: &str = "openai";
const KEK_HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const IDENTITY: &str =
    "SELECT tenant_id::text, workspace_id::text FROM fleet.fleets WHERE id = $1::uuid";
const PERSON: &str = "\
INSERT INTO identity.users(id,tenant_id,oidc_subject,email,created_at,updated_at)
VALUES($1::uuid,$2::uuid,$1,$1,now(),now())";
const ENTRY: &str = "\
INSERT INTO model.tenant_models(id,tenant_id,model_name,secret_ref,created_at,updated_at)
VALUES($1::uuid,$2::uuid,$3,$4,now(),now())";
const SECRET: &str = "\
INSERT INTO vault.secrets(id,workspace_id,key_name,dek_ciphertext,dek_nonce,dek_tag,secret_nonce,
secret_ciphertext,secret_tag,kek_version,meta_provider,meta_has_key,created_at,updated_at)
VALUES($1::uuid,$2::uuid,$3,$4,$5,$6,$7,$8,$9,$10,$11,true,now(),now())";

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
        running.fixtures.seed_wallet(&seat.tenant, FUNDED).await;
        seat
    }

    async fn seed(&self, database: &Db) {
        let model = format!("gpt-4o-{}", self.catalogue);
        let mut connection = database.acquire().await.expect("seat seed");
        sqlx::query(PERSON)
            .bind(self.owner.as_str())
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the tenant's owner");
        ModelSeed::new(self.catalogue.as_str(), PROVIDER, &model, 128_000)
            .rates(NANOS_PER_USD, NANOS_PER_USD, NANOS_PER_USD)
            .inference(RESPONSES_INFERENCE)
            .insert(&mut *connection)
            .await
            .expect("an isolated priced model");
        sqlx::query(ENTRY)
            .bind(self.entry.as_str())
            .bind(&self.tenant)
            .bind(&model)
            .bind(SECRET_REF)
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
        sqlx::query("DELETE FROM model.platform_models WHERE id = $1::uuid")
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
