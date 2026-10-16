//! Own-runner renewal and post-purge accounting use real HTTP and database state.

use super::super::{json_body, mint, send};
use super::fixture::Model;
use http::{Method, StatusCode};
use serde_json::{Value, json};

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn runner_renews_and_accounts_after_creator_deletes_private_text() {
    let model = Model::create().await;
    let r = &model.running;
    let root = r.path.trim_end_matches("/activity");
    let renew = format!("{root}/renew");
    let reports = format!("{root}/reports");
    let usage = json!({"knowledge":"known","calls":0,"input_tokens":0,"output_tokens":0,"runtime_ms":0,"provider_cost_nanos":"0"});
    let input = json!({"fence":"1","deletion_generation":"0","usage":usage});
    let extended = send(
        &r.router,
        Method::POST,
        &renew,
        Some(&r.token),
        &input.to_string(),
    )
    .await;
    assert_eq!(extended.status(), StatusCode::OK);
    let extended = json_body(extended).await;
    assert_eq!(
        extended.get("expires_at"),
        extended.get("model_access_expires_at")
    );
    let deleted = send(
        &r.router,
        Method::DELETE,
        &format!("{}/{}", r.f.collection(), r.draft),
        Some(&r.f.token),
        "",
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let input = json!({"kind":"accounting","fence":"1","deletion_generation":"0","report_id":mint(),"outcome":"completed","usage":usage});
    replay_accounting(&model, &reports, &input).await;
    let private: i64 =
        sqlx::query_scalar("SELECT count(*) FROM core.authoring_messages WHERE draft_id=$1::uuid")
            .bind(&r.draft)
            .fetch_one(&mut *r.f.db.acquire().await.expect("private read"))
            .await
            .expect("purged messages");
    assert_eq!(private, 0);
    assert_eq!(
        model.receipts().await,
        0,
        "no model dispatch was needed for accounting"
    );
    model.running.f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn renewal_and_report_refuse_person_identity_and_accounting_text() {
    let model = Model::create().await;
    let r = &model.running;
    let root = r.path.trim_end_matches("/activity");
    let usage = json!({"knowledge":"known","calls":0,"runtime_ms":0});
    let renew = json!({"fence":"1","deletion_generation":"0","usage":usage});
    let report = json!({"kind":"accounting","fence":"1","deletion_generation":"0","report_id":mint(),"outcome":"completed","usage":usage});
    for (suffix, body) in [("renew", renew), ("reports", report.clone())] {
        let reply = send(
            &r.router,
            Method::POST,
            &format!("{root}/{suffix}"),
            Some(&r.f.token),
            &body.to_string(),
        )
        .await;
        assert_eq!(reply.status(), StatusCode::UNAUTHORIZED);
    }
    let mut forged = report;
    forged
        .as_object_mut()
        .expect("report object")
        .insert("answer".into(), json!("private"));
    let reply = send(
        &r.router,
        Method::POST,
        &format!("{root}/reports"),
        Some(&r.token),
        &forged.to_string(),
    )
    .await;
    assert_eq!(reply.status(), StatusCode::BAD_REQUEST);
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM billing.authoring_report_receipts r JOIN fleet.authoring_attempts a ON a.id=r.attempt_id WHERE a.draft_id=$1::uuid")
        .bind(&r.draft).fetch_one(&mut *r.f.db.acquire().await.expect("receipt read")).await.expect("no effects on refusal");
    assert_eq!(count, 0);
    model.running.f.cleanup().await;
}

async fn replay_accounting(model: &Model, reports: &str, input: &Value) {
    let r = &model.running;
    for replayed in [false, true] {
        let ack = send(
            &r.router,
            Method::POST,
            reports,
            Some(&r.token),
            &input.to_string(),
        )
        .await;
        assert_eq!(ack.status(), StatusCode::OK);
        assert_eq!(
            ack.headers()
                .get("cache-control")
                .expect("private cache policy"),
            "no-store"
        );
        let ack = json_body(ack).await;
        assert_eq!(ack.get("settled"), Some(&Value::Bool(true)));
        assert_eq!(ack.get("replayed"), Some(&Value::Bool(replayed)));
        assert!(ack.get("answer").is_none());
    }
}
