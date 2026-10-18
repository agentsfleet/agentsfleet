//! Additive inference settings leave existing catalogue rows explicitly unconfigured.

use super::{cutover_rows::through, *};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Postgres"]
async fn populated_models_survive_inference_and_native_reply_migrations_without_fabricated_settings()
 {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new()
        .with_migrations(through(955))
        .run(&db)
        .await
        .expect("prior schema");
    let id = afd_db::test_util::mint_id();
    sqlx::query("INSERT INTO core.model_library(id,model_id,provider,context_cap_tokens,input_nanos_per_mtok,cached_input_nanos_per_mtok,output_nanos_per_mtok,created_at,updated_at) VALUES($1::uuid,'saved-model','saved-provider',128000,1,1,2,3,4)")
        .bind(&id).execute(&mut *db.acquire().await.unwrap()).await.expect("existing model");
    let migrated = Migrator::new()
        .with_migrations(through(957))
        .run(&db)
        .await
        .expect("forward migration");
    assert_eq!(migrated.applied, [956, 957]);
    let kept: (String, i32, i64, i64, Option<String>) = sqlx::query_as("SELECT model_id,context_cap_tokens,output_nanos_per_mtok,updated_at,inference::text FROM core.model_library WHERE id=$1::uuid")
        .bind(&id).fetch_one(&mut *db.acquire().await.unwrap()).await.expect("preserved model");
    assert_eq!(kept, ("saved-model".into(), 128_000, 2, 4, None));
    let native: String = sqlx::query_scalar("SELECT is_nullable FROM information_schema.columns WHERE table_schema='fleet' AND table_name='authoring_dispatches' AND column_name='native_reply'")
        .fetch_one(&mut *db.acquire().await.unwrap()).await.expect("new private field");
    assert_eq!(native, "YES");
    assert_eq!(
        Migrator::new()
            .with_migrations(through(957))
            .run(&db)
            .await
            .expect("idempotent migration")
            .applied,
        [] as [i32; 0]
    );
    db.close().await;
    database.cleanup().await;
}
