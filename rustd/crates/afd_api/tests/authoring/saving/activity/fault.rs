//! A workspace-scoped receipt refusal; its nontransactional counter proves it fired.

use super::super::super::mint;
use afd_core::id::Uuid7;
use sqlx::{AssertSqlSafe, Connection as _};
use std::time::Duration;

const DDL_LOCK: i64 = 0x6166_645f_6974;
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);
const NOT_REMOVED: &str = "test_receipt_refusal_not_removed";

pub(in crate::authoring::saving) struct Refusal {
    url: String,
    name: String,
}

impl Refusal {
    pub(in crate::authoring::saving) async fn install(
        url: String,
        workspace: &Uuid7,
        kind: &str,
    ) -> Self {
        Self::scoped(url, workspace, kind, false).await
    }

    pub(in crate::authoring::saving) async fn at_commit(
        url: String,
        workspace: &Uuid7,
        kind: &str,
    ) -> Self {
        Self::scoped(url, workspace, kind, true).await
    }

    async fn scoped(url: String, workspace: &Uuid7, kind: &str, at_commit: bool) -> Self {
        let name = format!("receipt_refusal_{}", mint().as_str().replace('-', ""));
        let guard = Self { url, name };
        let mut owner = sqlx::PgConnection::connect(&guard.url)
            .await
            .expect("fixture owner connection");
        let mut tx = owner.begin().await.expect("fixture DDL transaction");
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(DDL_LOCK)
            .execute(&mut *tx)
            .await
            .expect("serialize fixture DDL");
        let trigger = if at_commit {
            format!(
                "CREATE CONSTRAINT TRIGGER {} AFTER INSERT ON fleet.events DEFERRABLE INITIALLY DEFERRED",
                guard.name
            )
        } else {
            format!(
                "CREATE TRIGGER {} BEFORE INSERT ON fleet.events",
                guard.name
            )
        };
        // The identifier, UUID and receipt kind are fixture-owned, never request input.
        for statement in [
            format!("CREATE SEQUENCE public.{}_hits", guard.name),
            format!(
                "GRANT USAGE, SELECT ON SEQUENCE public.{}_hits TO api_runtime",
                guard.name
            ),
            format!(
                "CREATE FUNCTION public.{0}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
                PERFORM nextval('public.{0}_hits');
                RAISE EXCEPTION 'injected receipt refusal'; END $$",
                guard.name
            ),
            format!(
                "{trigger} FOR EACH ROW
                WHEN (NEW.workspace_id = '{1}'::uuid AND NEW.event_type = 'activity'
                AND NEW.request_json->>'kind' = '{2}') EXECUTE FUNCTION public.{0}()",
                guard.name,
                workspace.as_str(),
                kind
            ),
        ] {
            sqlx::query(AssertSqlSafe(statement))
                .execute(&mut *tx)
                .await
                .expect("install scoped refusal");
        }
        tx.commit().await.expect("commit fixture DDL");
        guard
    }

    pub(in crate::authoring::saving) async fn assert_fired_once(&self) {
        let mut owner = sqlx::PgConnection::connect(&self.url)
            .await
            .expect("counter connection");
        let hits: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT CASE WHEN is_called THEN last_value ELSE 0 END FROM public.{}_hits",
            self.name
        )))
        .fetch_one(&mut owner)
        .await
        .expect("nontransactional fault counter");
        assert_eq!(
            hits, 1,
            "the selected receipt insertion failed exactly once"
        );
    }
}

impl Drop for Refusal {
    fn drop(&mut self) {
        let (url, name) = (self.url.clone(), self.name.clone());
        let removed = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("fixture cleanup runtime")
                .block_on(async {
                    tokio::time::timeout(CLEANUP_TIMEOUT, remove(&url, &name)).await
                })
        })
        .join();
        if !matches!(removed, Ok(Ok(Ok(())))) {
            if std::thread::panicking() {
                tracing::warn!(event = NOT_REMOVED, trigger = %self.name);
            } else {
                panic!("receipt refusal cleanup failed or exceeded its deadline");
            }
        }
    }
}

async fn remove(url: &str, name: &str) -> sqlx::Result<()> {
    let mut owner = sqlx::PgConnection::connect(url).await?;
    let mut tx = owner.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(DDL_LOCK)
        .execute(&mut *tx)
        .await?;
    for statement in [
        format!("DROP TRIGGER IF EXISTS {name} ON fleet.events"),
        format!("DROP FUNCTION IF EXISTS public.{name}()"),
        format!("DROP SEQUENCE IF EXISTS public.{name}_hits"),
    ] {
        sqlx::query(AssertSqlSafe(statement))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}
