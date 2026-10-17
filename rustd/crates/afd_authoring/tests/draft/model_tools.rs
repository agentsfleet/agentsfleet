//! A usable text endpoint need not support the proposal tool setup chat requires.

use super::{
    fixture::{Fixture, now},
    mint,
};
use afd_authoring::types::{ModelPurpose, ModelsQuery};
use afd_core::error_code;
use afd_wire::inference::{InferenceApi, ModelInference};

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn tool_less_chat_routes_remain_runtime_choices_but_cannot_create_setup_chats() {
    for dialect in ["perplexity", "hyperbolic", "mira"] {
        let f = Fixture::create().await;
        let settings = ModelInference {
            api: InferenceApi::Chat,
            base_url: "https://models.example.com/v1".into(),
            dialect: Some(dialect.into()),
            max_output_tokens: 8192,
        };
        sqlx::query("UPDATE core.model_library SET inference=$2 WHERE id=$1::uuid")
            .bind(f.catalogue.first().expect("owned catalogue").as_str())
            .bind(sqlx::types::Json(settings))
            .execute(&mut *f.connection().await)
            .await
            .expect("configured credentialed text route");
        assert_purpose_filters(&f).await;
        let error = f
            .drafts
            .create(f.access(), f.request(mint(), Some("Build a setup")), now())
            .await
            .expect_err("proposal tool required before creating private state");
        assert_eq!(error.code(), error_code::AUTHORING_MODEL);
        assert_eq!(f.counts().await, (0, 0, 0));
        f.cleanup().await;
    }
}

async fn assert_purpose_filters(f: &Fixture) {
    for purpose in [
        None,
        Some(ModelPurpose::Authoring),
        Some(ModelPurpose::Runtime),
    ] {
        let page = f
            .drafts
            .models(
                f.access(),
                ModelsQuery {
                    limit: 100,
                    purpose,
                    starting_after: None,
                },
            )
            .await
            .expect("public model catalogue");
        let selected = page
            .items
            .iter()
            .find(|row| row.reference == f.model.as_str());
        if matches!(purpose, Some(ModelPurpose::Authoring)) {
            assert!(
                selected.is_none(),
                "tool-less route cannot be offered for setup chat"
            );
            assert_eq!(page.items.len(), 1, "tool-capable owner model retained");
        } else {
            let selected = selected.expect("text inference remains configured");
            assert!(!selected.eligibility.authoring);
            assert!(selected.eligibility.runtime);
        }
    }
}
