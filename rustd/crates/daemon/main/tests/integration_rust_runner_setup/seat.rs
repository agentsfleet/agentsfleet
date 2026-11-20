//! A person in the scenario's workspace who shapes a fleet: an owner, a
//! priced model of a chosen window behind a sealed key, and the draft they
//! talk to through the production create and send paths.

use af_core::{clock, id::Uuid7, money::NANOS_PER_USD};
use af_crypto::{aad::Aad, entropy::Entropy, envelope::Sealer, secret::Kek};
use afd_db::test_util::{ModelSeed, mint_id};
use afd_setup::types::{CreateDraft, Draft, SendMessage, Source};
use afd_setup::{Access, Drafts};

use crate::e2e::{GOOD_KEK, Scenario};

/// The provider the seated model is filed under.
const PROVIDER: &str = "openai";
/// The key name the seated model's secret is stored under.
const SECRET_REF: &str = "setup-model";
const PERSON: &str = "\
INSERT INTO identity.users(id,tenant_id,oidc_subject,email,created_at,updated_at)
VALUES($1,$2,$1,$1,now(),now())";
const ENTRY: &str = "\
INSERT INTO model.tenant_models(id,tenant_id,model_name,secret_ref,workspace_id,created_at,updated_at)
VALUES($1,$2,$3,$4,$5,now(),now())";
const SECRET: &str = "\
INSERT INTO vault.secrets(id,workspace_id,key_name,dek_ciphertext,dek_nonce,dek_tag,secret_nonce,
secret_ciphertext,secret_tag,kek_version,meta_provider,meta_has_key,created_at,updated_at)
VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,true,now(),now())";

/// The scenario's person and the model their draft runs on.
pub(super) struct Seat {
    owner: Uuid7,
    workspace: Uuid7,
    entry: Uuid7,
    catalogue: Uuid7,
    /// The model's name, as a request names it.
    pub(super) model: String,
    pub(super) drafts: Drafts,
}

impl Seat {
    /// A person in `scene`'s workspace, whose model has a window of
    /// `context_tokens` and writes at most `output_tokens` a step.
    pub(super) async fn create(scene: &Scenario, context_tokens: i32, output_tokens: u32) -> Self {
        let catalogue = mint_id();
        let seat = Self {
            owner: mint_id(),
            workspace: scene.workspace.clone(),
            entry: mint_id(),
            model: format!("gpt-setup-{catalogue}"),
            catalogue,
            drafts: Drafts::new(scene.booted.database.clone(), Entropy::new())
                .with_bus(scene.booted.bus.clone()),
        };
        seat.seed(scene, context_tokens, output_tokens).await;
        seat
    }

    async fn seed(&self, scene: &Scenario, context_tokens: i32, output_tokens: u32) {
        let inference = format!(
            r#"{{"api":"responses","base_url":"https://models.example.com/v1","max_output_tokens":{output_tokens}}}"#
        );
        let mut connection = scene
            .booted
            .database
            .acquire()
            .await
            .expect("a pooled connection");
        sqlx::query(PERSON)
            .bind(&self.owner)
            .bind(&scene.tenant)
            .execute(&mut *connection)
            .await
            .expect("the person shaping the fleet");
        ModelSeed::new(&self.catalogue, PROVIDER, &self.model, context_tokens)
            .rates(NANOS_PER_USD, NANOS_PER_USD, NANOS_PER_USD)
            .inference(&inference)
            .insert(&mut *connection)
            .await
            .expect("a priced model of the chosen window");
        sqlx::query(ENTRY)
            .bind(&self.entry)
            .bind(&scene.tenant)
            .bind(&self.model)
            .bind(SECRET_REF)
            .bind(&self.workspace)
            .execute(&mut *connection)
            .await
            .expect("the person's registered model");
        let kek = Kek::from_hex(GOOD_KEK).expect("the scenario's key");
        let sealed = Sealer::new()
            .seal(
                &kek,
                &Aad::new(self.workspace.as_str(), SECRET_REF),
                br#"{"provider":"openai","api_key":"fixture-setup-key"}"#,
            )
            .expect("the fixture key seals");
        sqlx::query(SECRET)
            .bind(mint_id())
            .bind(&self.workspace)
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
            .expect("the sealed key");
    }

    fn access(&self) -> Access<'_> {
        Access {
            workspace: &self.workspace,
            subject: self.owner.as_str(),
        }
    }

    /// A blank draft whose first turn says `message`.
    pub(super) async fn start(&self, message: &str) -> Draft {
        let request = CreateDraft {
            save: false,
            source: Source::Blank,
            authoring_model_ref: self.entry.to_string(),
            message: Some(message.to_owned()),
            operation_id: mint_id(),
        };
        self.drafts
            .create(self.access(), request, clock::now())
            .await
            .expect("the draft and its first turn")
            .draft
    }

    /// Sends `message` as `draft`'s next turn.
    pub(super) async fn send(&self, draft: &Draft, message: &str) {
        let request = SendMessage {
            message: message.to_owned(),
            save: false,
            operation_id: mint_id(),
        };
        self.drafts
            .send(self.access(), &draft.id, request, clock::now())
            .await
            .expect("the draft's next turn");
    }

    /// Removes the draft, so no later scenario's runner is assigned its
    /// turns, and the catalogue row; the lane reset owns the rest.
    pub(super) async fn remove(self, scene: &Scenario, draft: &Draft) {
        let mut connection = scene
            .booted
            .database
            .acquire()
            .await
            .expect("a pooled connection");
        sqlx::query("DELETE FROM setup.drafts WHERE id = $1")
            .bind(&draft.id)
            .execute(&mut *connection)
            .await
            .expect("the draft goes");
        ModelSeed::remove(&mut *connection, &self.catalogue)
            .await
            .expect("the catalogue row goes");
    }
}
