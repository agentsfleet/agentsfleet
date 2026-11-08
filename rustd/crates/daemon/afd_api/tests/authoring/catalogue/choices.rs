//! The caller cannot substitute their own account's credentials or prices.

use super::super::seed::{INPUT_NANOS_PER_MTOK, OUTPUT_NANOS_PER_MTOK};
use super::{Fixture, list};
use serde_json::json;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn model_choices_use_workspace_owner_and_exact_priced_eligibility() {
    let f = Fixture::create().await;
    let other = Fixture::create().await;
    let page = list(&f, "?purpose=authoring").await;
    let items = page["items"].as_array().expect("bounded model page");
    let own = items
        .iter()
        .find(|item| item["ref"] == json!(f.model))
        .expect("owner's configured selection");
    assert_eq!(own["eligibility"], json!({"authoring":true,"runtime":true}));
    assert_eq!(
        own["pricing"]["input_nanos_per_million"],
        INPUT_NANOS_PER_MTOK.to_string()
    );
    assert_eq!(
        own["pricing"]["output_nanos_per_million"],
        OUTPUT_NANOS_PER_MTOK.to_string()
    );
    assert_eq!(
        own["pricing"]["runtime_nanos_per_second"],
        afd_billing::RUN_NANOS_PER_SEC.to_string()
    );
    assert_eq!(own["max_output_tokens"], 8192);
    assert!(!items.iter().any(|item| item["ref"] == json!(other.model)));
    assert!(!page.to_string().contains("secret_ref"));
    assert!(!page.to_string().contains("fixture-authoring-key"));
    assert!(page["total"].is_null());
    other.cleanup().await;
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn missing_credential_metadata_removes_the_choice_without_fallback() {
    let f = Fixture::create().await;
    sqlx::query("UPDATE vault.secrets SET meta_has_key=false WHERE workspace_id=$1::uuid")
        .bind(f.workspace.as_str())
        .execute(&mut *f.db.acquire().await.expect("fixture connection"))
        .await
        .expect("remove usable metadata");
    let page = list(&f, "?purpose=authoring").await;
    assert!(
        !page["items"]
            .as_array()
            .expect("page")
            .iter()
            .any(|item| item["ref"] == json!(f.model))
    );
    f.cleanup().await;
}
