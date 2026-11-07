//! A disabled model forces an idle replacement; a queued one cannot be swapped.

use super::{claim, fixture, funded, mint, now, queue};
use afd_authoring::types::{DraftPatch, MessageState, SendMessage};
use afd_core::{
    error_code::{AUTHORING_MODEL, AUTHORING_STATE},
    id::Uuid7,
};
use afd_credential::provider::PLATFORM_MODEL_REF;
use serde_json::json;

fn patch(value: serde_json::Value) -> DraftPatch {
    serde_json::from_value(value).expect("typed patch")
}

fn replace(model: &Uuid7) -> DraftPatch {
    patch(json!({ "authoring_model_ref": model }))
}

async fn etag(f: &super::Fixture, draft: &Uuid7) -> String {
    f.drafts
        .read(f.access(), draft)
        .await
        .expect("creator reload")
        .etag
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn disabled_model_refuses_queued_replacement_and_recovers_after_idle_replacement() {
    let f = funded(1).await;
    let draft = queue(&f, "Selected model is disabled mid-draft").await;
    sqlx::query("DELETE FROM model.tenant_models WHERE id=$1::uuid")
        .bind(f.model.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("disable owned entry");
    let queued = etag(&f, &draft.id).await;
    let error = f
        .drafts
        .patch(
            f.access(),
            &draft.id,
            Some(&queued),
            replace(&f.other_model),
            now(),
        )
        .await
        .expect_err("a queued request keeps the model it was accepted with");
    assert_eq!(error.code(), AUTHORING_STATE);
    assert!(claim(&f, &draft).await.is_none());
    let failed = fixture::failure(&f, &draft).await;
    assert_eq!(failed.state, MessageState::Failed);
    let failure = failed.failure.expect("durable bounded failure");
    assert_eq!(failure.error_code, AUTHORING_MODEL);
    assert_eq!(failure.recovery_action, "choose_authoring_model");
    assert_eq!(
        fixture::attempts(&f).await,
        0,
        "no obligation for a refusal"
    );
    let idle = f.drafts.read(f.access(), &draft.id).await.expect("reload");
    // Deleting a tenant model sets every pointer to it NULL, which reads as
    // the platform default; this fixture configures none, so it is no escape.
    assert_eq!(idle.authoring_model_ref, PLATFORM_MODEL_REF);
    for (forced, why) in [
        (
            patch(json!({"title":"rename only"})),
            "any edit needs a usable model",
        ),
        (
            replace(&f.model),
            "the disabled model cannot be chosen again",
        ),
    ] {
        let error = f
            .drafts
            .patch(f.access(), &draft.id, Some(&idle.etag), forced, now())
            .await
            .expect_err(why);
        assert_eq!(error.code(), AUTHORING_MODEL, "{why}");
    }
    let replaced = f
        .drafts
        .patch(
            f.access(),
            &draft.id,
            Some(&idle.etag),
            replace(&f.other_model),
            now(),
        )
        .await
        .expect("idle replacement after the durable failure");
    assert_eq!(replaced.authoring_model_ref, f.other_model.to_string());
    assert_recovers(&f, &draft.id).await;
    f.cleanup().await;
}

/// The next request runs on the replacement; the failure stays on reload.
async fn assert_recovers(f: &super::Fixture, draft: &Uuid7) {
    let accepted = f
        .drafts
        .send(
            f.access(),
            draft,
            SendMessage {
                save: false,
                message: "Try again on the replacement".to_owned(),
                operation_id: mint(),
            },
            now(),
        )
        .await
        .expect("idle draft accepts the next request");
    assert_eq!(accepted.sequence, "2");
    let reloaded = f.drafts.read(f.access(), draft).await.expect("reload");
    let lease = claim(f, &reloaded).await.expect("replacement admitted");
    assert_eq!(lease.sequence, "2");
    assert_eq!(lease.model.reference, f.other_model.to_string());
    assert_eq!(fixture::attempts(f).await, 1);
    let kept = fixture::failure(f, &reloaded).await;
    assert_eq!(
        kept.failure.expect("first failure survives").error_code,
        AUTHORING_MODEL
    );
}
