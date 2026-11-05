//! Real setup, catalogue, vault, billing and candidate persistence around two provider boundaries.

use afd_admin::test_util::DecisionModelFixture;
use afd_authoring::{ModelCalls, types::Source};
use afd_crypto::{entropy::Entropy, secret::Kek};
use afd_vault::{SecretBody, SecretName, Vault};
use serde_json::{Value, json};
use std::sync::Arc;

use super::super::{
    super::{attempt::Attempt, definition, fixture::Fixture},
    fixture::{self, Provider},
};
use super::server::Server;

pub(super) const SOAK: &str = "Two implementations are equally fast. How should we choose between them? Explain in at most thirty words without changing the setup.";

pub(super) struct Case {
    pub f: Fixture,
    pub attempt: Attempt,
    pub calls: ModelCalls,
    pub provider: Arc<Provider>,
    pub model: DecisionModelFixture,
}

impl Case {
    pub(super) async fn create(server: &Server, text: &str) -> Self {
        // Admin edits advance the whole catalogue generation. Keep them private
        // to this case and finish them before pinning the chat model snapshot.
        let f = Fixture::isolated().await;
        let mut model = DecisionModelFixture::create(&f.db)
            .await
            .expect("reviewed decision model fixture");
        let mut details = model.model.details.clone();
        let decision = details.decision.as_mut().expect("reviewed decision");
        decision
            .configuration
            .profile
            .endpoint
            .clone_from(&server.endpoint);
        decision.configuration.profile.revision = "setup-edit-reviewed".into();
        let price = decision.prices.first_mut().expect("decision base price");
        price.input_nanos_per_mtok = 1_000_000;
        price.cached_input_nanos_per_mtok = 2_000_000;
        price.cache_write_nanos_per_mtok = Some(3_000_000);
        price.output_nanos_per_mtok = 4_000_000;
        assert!(
            model
                .models
                .update(&model.model.id, details.clone(), afd_core::clock::now())
                .await
                .expect("freeze fixture endpoint")
        );
        model.model.details = details;
        let attempt = Attempt::start(
            &f,
            f.access(),
            Source::Definition {
                definition: Box::new(definition()),
            },
        )
        .await;
        sqlx::query("UPDATE authoring.messages SET user_text=$2 WHERE draft_id=$1::uuid")
            .bind(attempt.draft.id.as_str())
            .bind(text)
            .execute(&mut *f.connection().await)
            .await
            .expect("accepted newest message");
        sqlx::query(
            "UPDATE runner.runners SET decision_platform_model_id=$2::uuid WHERE id=$1::uuid",
        )
        .bind(f.runner.as_str())
        .bind(model.model.id.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("configured decision model");
        credential(&f).await;
        let mut stub = Provider::known();
        let base = attempt.draft.candidate.as_ref().expect("source candidate");
        *stub.response.get_mut("output").expect("fixture output") = json!([
            {"type":"message","content":[{"type":"output_text","text":"Choose the clearer implementation, weighing maintainability, correctness, and operational risk. Measure again only if those trade-offs remain unresolved."}]},
            {"type":"function_call","name":"propose_setup","call_id":"unasked-edit","arguments":json!({"proposal":{
                "kind":"edit","base_candidate_id":base.id,"base_candidate_revision":base.revision,
                "config_ops":[],"instruction_edits":[{"kind":"append","text":"\nPrefer clarity."}]
            }}).to_string()}
        ]);
        let provider = Arc::new(stub);
        let calls = fixture::calls(&f, &attempt, Arc::clone(&provider))
            .await
            .with_decisions(f.workspace.clone(), server.client.clone());
        Self {
            f,
            attempt,
            calls,
            provider,
            model,
        }
    }

    pub(super) async fn journal(&self) -> (String, Option<i64>, bool) {
        sqlx::query_as("SELECT disposition, charged_nanos, accounted_at IS NOT NULL FROM authoring.edit_decisions WHERE attempt_id=$1::uuid")
            .bind(self.attempt.id.as_str()).fetch_one(&mut *self.f.connection().await).await.expect("persisted decision")
    }

    pub(super) async fn finish(self) {
        sqlx::query("UPDATE runner.runners SET decision_platform_model_id=NULL WHERE id=$1::uuid")
            .bind(self.f.runner.as_str())
            .execute(&mut *self.f.connection().await)
            .await
            .expect("remove fixture assignment");
        self.model
            .finish()
            .await
            .expect("decision model fixture lifecycle");
        self.f.cleanup().await;
    }
}

async fn credential(f: &Fixture) {
    let kek = Kek::from_hex("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
        .expect("fixture key");
    let vault = Vault::new(f.db.clone(), Arc::new(kek), Entropy::new());
    let raw = serde_json::value::RawValue::from_string(
        json!({"api_key": (["setup","decision","fixture"].join("-"))}).to_string(),
    )
    .expect("secret JSON");
    vault
        .create(
            &f.workspace,
            &SecretName::parse("typesafe").expect("fixed secret bag"),
            &SecretBody::parse(&raw).expect("secret shape"),
            afd_core::clock::now(),
        )
        .await
        .expect("seal platform fixture credential");
}

pub(super) fn reply(noul: f64) -> Value {
    json!({"model":"evaluated-fixture-model","usage":{"input_tokens":100,"output_tokens":3},
        "answers":{"setup_edit":{"type":"noul","noul":noul}}})
}
