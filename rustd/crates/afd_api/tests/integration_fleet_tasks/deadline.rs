//! Connected SQL lock waits time out and leave subsequent task reads usable.
#![expect(clippy::expect_used, reason = "test preconditions fail loudly")]

use std::panic::AssertUnwindSafe;
use std::time::Duration;

use afd_auth::scope::{Scope, ScopeSet};
use afd_db::{Db, config::DbRole, test_util::TestDatabase};
use futures_util::FutureExt as _;
use http::{Method, StatusCode};
use sqlx::{Acquire as _, PgConnection};

use super::fixture::{SUBJECT, Tasks};
use crate::harness::{Fleet, code_of, json_body, send};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn connected_task_reads_cancel_both_scope_and_page_lock_waits() {
    let lane = TestDatabase::create().await;
    let result = AssertUnwindSafe(exercise(&lane)).catch_unwind().await;
    tokio::time::timeout(Duration::from_secs(10), lane.cleanup())
        .await
        .expect(
            "isolated database disposal runs within its bound, including after assertion failure",
        );
    if let Err(failure) = result {
        std::panic::resume_unwind(failure);
    }
}

async fn exercise(lane: &TestDatabase) {
    let migrator = lane.open(DbRole::Migrator, &[]).await;
    afd_db::Migrator::new()
        .run(&migrator)
        .await
        .expect("isolated schema migrates");
    migrator.close().await;
    let fixture = Tasks::create(lane).await;
    let router = Fleet::live(
        fixture.base.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&[Scope::FleetRead]),
    )
    .with_owned_workspace(fixture.base.workspace.clone())
    .router();
    let migrator = fixture.base.lane.open(DbRole::Migrator, &[]).await;
    let mut connection = migrator.acquire().await.expect("lock owner");
    let blocker: i32 = sqlx::query_scalar(include_str!("backend_pid.sql"))
        .fetch_one(&mut *connection)
        .await
        .expect("lock identity");
    page_waits(&router, &fixture, &mut connection, blocker).await;
    scope_waits(&router, &fixture, &mut connection, blocker).await;
    drop(connection);
    migrator.close().await;
    fixture.base.cleanup().await;
}

async fn page_waits(
    router: &axum::Router,
    fixture: &Tasks<'_>,
    connection: &mut PgConnection,
    blocker: i32,
) {
    let mut lock = connection.begin().await.expect("lock transaction");
    sqlx::query(include_str!("lock.sql"))
        .execute(&mut *lock)
        .await
        .expect("isolated task table locks");
    let collection = fixture.collection();
    let detail = fixture.item(0);
    let reads = tokio::time::timeout(Duration::from_secs(8), async {
        tokio::join!(
            send(
                router,
                Method::GET,
                &collection,
                Some(&fixture.base.token),
                ""
            ),
            send(router, Method::GET, &detail, Some(&fixture.base.token), ""),
            blocked(&fixture.base.database, blocker, 2),
        )
    })
    .await;
    lock.rollback().await.expect("blocker releases");
    let (listing, item, observed) = reads.expect("neither connected read may wait indefinitely");
    assert!(
        observed,
        "both reads reached the actual blocked SQL statements"
    );
    for response in [listing, item] {
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            code_of(response).await,
            afd_core::error_code::INTERNAL_DB_UNAVAILABLE.as_str()
        );
    }
    for path in [&collection, &detail] {
        let response = send(router, Method::GET, path, Some(&fixture.base.token), "").await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "pool remains usable after cancellation"
        );
        let body = json_body(response).await;
        assert!(body.is_object());
    }
}

async fn scope_waits(
    router: &axum::Router,
    fixture: &Tasks<'_>,
    connection: &mut PgConnection,
    blocker: i32,
) {
    let collection = fixture.collection();
    let mut lock = connection.begin().await.expect("scope blocker transaction");
    sqlx::query(include_str!("lock_fleet.sql"))
        .execute(&mut *lock)
        .await
        .expect("isolated fleet table lock");
    let scope_wait = tokio::time::timeout(Duration::from_secs(8), async {
        tokio::join!(
            send(
                router,
                Method::GET,
                &collection,
                Some(&fixture.base.token),
                ""
            ),
            blocked(&fixture.base.database, blocker, 1),
        )
    })
    .await;
    lock.rollback().await.expect("scope blocker releases");
    let (response, observed) = scope_wait.expect("scope query is bounded too");
    assert!(observed, "collection reached its blocked scope check");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        code_of(response).await,
        afd_core::error_code::INTERNAL_DB_UNAVAILABLE.as_str()
    );
    let response = send(
        router,
        Method::GET,
        &collection,
        Some(&fixture.base.token),
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK, "scope read recovers");
    assert_eq!(crate::harness::items(&json_body(response).await).len(), 3);
}

async fn blocked(database: &Db, blocker: i32, expected: i64) -> bool {
    let mut observer = database.acquire().await.expect("autocommit lock observer");
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let count: i64 = sqlx::query_scalar(include_str!("blocked.sql"))
                .bind(blocker)
                .fetch_one(&mut *observer)
                .await
                .expect("blocked read count");
            if count >= expected {
                return true;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or(false)
}
