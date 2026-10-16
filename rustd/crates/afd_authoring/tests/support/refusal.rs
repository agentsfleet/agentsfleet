//! A Postgres trigger that refuses one fleet's chosen updates, removed on drop.
//!
//! Setup faults worth proving happen inside a committed sequence: the
//! activation update, or the receipt saved after an append. A trigger scoped
//! to one fixture fleet makes that single statement fail exactly where
//! production would see a lost connection or an aborted transaction, while
//! every other fleet in the shared lane is untouched.
//!
//! `#[path]`-included by the authoring suite and the router suite, so both
//! drive the same fault rather than two copies drifting apart.

use sqlx::{AssertSqlSafe, Connection as _};

/// The event a trigger that could not be removed is logged under.
const NOT_REMOVED: &str = "test_refusal_not_removed";
/// One advisory lock every fixture trigger install and removal takes first.
///
/// Two `DROP TRIGGER`s on one table, run at once by two tests in the shared
/// lane, can deadlock each other; Postgres then aborts one, the trigger stays,
/// and the next test step meets a refusal it never asked for. Taking this
/// lock in the same transaction serialises fixture DDL, so that cycle cannot
/// form. The value is arbitrary; only its being one value matters.
const FIXTURE_DDL_LOCK: i64 = 0x6166_645f_6974;
/// Takes [`FIXTURE_DDL_LOCK`] for the rest of the transaction.
const TAKE_DDL_LOCK: &str = "SELECT pg_advisory_xact_lock($1)";

/// One installed refusal: its trigger and function, dropped with the guard.
#[must_use = "the trigger is removed when the guard drops, so bind it for the fault's life"]
pub(crate) struct Refusal {
    url: String,
    name: String,
    table: &'static str,
}

impl Refusal {
    /// Refuses every `UPDATE` on `table` whose row satisfies `when`.
    ///
    /// `kind` and `fleet` name the trigger, so two faults on one fleet never
    /// collide. Both, and `when`, come from the fixture's own minted IDs and
    /// constant SQL, never from input, which is what makes interpolating them
    /// safe.
    pub(crate) async fn install(
        url: &str,
        table: &'static str,
        kind: &str,
        fleet: &str,
        when: &str,
    ) -> Self {
        let guard = Self {
            url: url.to_owned(),
            name: format!("afd_it_{kind}_{}", fleet.replace('-', "")),
            table,
        };
        let name = &guard.name;
        let mut owner = sqlx::PgConnection::connect(url)
            .await
            .expect("the lane owner installs faults");
        let mut tx = owner.begin().await.expect("install transaction");
        sqlx::query(TAKE_DDL_LOCK)
            .bind(FIXTURE_DDL_LOCK)
            .execute(&mut *tx)
            .await
            .expect("serialise fixture DDL");
        for statement in [
            format!(
                "CREATE FUNCTION public.{name}() RETURNS trigger LANGUAGE plpgsql AS $$ \
                 BEGIN RAISE EXCEPTION 'fixture refusal {name}'; END $$"
            ),
            format!(
                "CREATE TRIGGER {name} BEFORE UPDATE ON {table} FOR EACH ROW \
                 WHEN ({when}) EXECUTE FUNCTION public.{name}()"
            ),
        ] {
            sqlx::query(AssertSqlSafe(statement))
                .execute(&mut *tx)
                .await
                .expect("install the fixture refusal");
        }
        tx.commit().await.expect("install the fixture refusal");
        guard
    }
}

impl Drop for Refusal {
    fn drop(&mut self) {
        // A thread with a runtime of its own: a drop cannot await, and
        // blocking the test's runtime on its own connection would hang a
        // current-thread test.
        let (url, name, table) = (self.url.clone(), self.name.clone(), self.table);
        match std::thread::spawn(move || remove(&url, &name, table)).join() {
            Ok(Ok(())) => {}
            Ok(Err(cause)) => {
                tracing::warn!(event = NOT_REMOVED, trigger = %self.name, error = %cause);
            }
            Err(_panicked) => tracing::warn!(event = NOT_REMOVED, trigger = %self.name),
        }
    }
}

/// Drops the trigger `name` on `table` and its function.
fn remove(url: &str, name: &str, table: &str) -> sqlx::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a removal runtime starts");
    runtime.block_on(async {
        let mut owner = sqlx::PgConnection::connect(url).await?;
        let mut tx = owner.begin().await?;
        sqlx::query(TAKE_DDL_LOCK)
            .bind(FIXTURE_DDL_LOCK)
            .execute(&mut *tx)
            .await?;
        for statement in [
            format!("DROP TRIGGER IF EXISTS {name} ON {table}"),
            format!("DROP FUNCTION IF EXISTS public.{name}()"),
        ] {
            sqlx::query(AssertSqlSafe(statement))
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await
    })
}
