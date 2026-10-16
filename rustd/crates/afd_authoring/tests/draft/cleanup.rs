//! Failure paths remove their uniquely owned rows even when assertions unwind.

use super::{fixture::Fixture, mint};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
#[expect(
    clippy::panic,
    reason = "inject an assertion failure to exercise unwind cleanup"
)]
async fn fixture_unwind_removes_only_registered_account_and_private_rows() {
    let f = Fixture::create().await;
    f.drafts
        .create(
            f.access(),
            f.request(mint(), Some("private test text")),
            super::fixture::now(),
        )
        .await
        .expect("owned private rows");
    let db = f.db.clone();
    let tenant = f.tenant.clone();
    let workspace = f.workspace.clone();
    let failed = tokio::spawn(async move {
        let _fixture = f;
        panic!("injected fixture assertion failure");
    })
    .await
    .expect_err("worker must unwind");
    assert!(failed.is_panic());
    let remaining: (i64, i64, i64) = sqlx::query_as(
        "SELECT \
        (SELECT count(*) FROM core.tenants WHERE id=$1::uuid), \
        (SELECT count(*) FROM core.authoring_drafts WHERE workspace_id=$2::uuid), \
        (SELECT count(*) FROM core.authoring_replay_receipts WHERE workspace_id=$2::uuid)",
    )
    .bind(tenant.as_str())
    .bind(workspace.as_str())
    .fetch_one(&mut *db.acquire().await.expect("observer connection"))
    .await
    .expect("observe completed unwind cleanup");
    assert_eq!(remaining, (0, 0, 0));
    db.close().await;
}
