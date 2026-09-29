//! The platform provider default a suite seeds, removed by the suite that
//! created it.
//!
//! `core.platform_provider_defaults` is keyed by provider, so the row is one
//! per deployment and every test in a run that needs it shares it: the first
//! to arrive writes it (`ON CONFLICT DO NOTHING`) and the rest reuse it. The
//! row used to outlive the run, pointing at a workspace whose key the suite's
//! own cleanup had removed, and every later run that reused it failed to
//! resolve a provider.
//!
//! So a test holds a [`PlatformDefault`] while it needs the row. Holders are
//! counted per process; when the last one drops — on success or on a panic,
//! because a drop runs while unwinding — the row is deleted, but only when
//! this process inserted it. A default that already existed, from an operator
//! or another process, is never touched. While the delete runs, a new holder
//! waits for it rather than reusing a row that is about to go.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use sqlx::Connection as _;

use super::lane_url;
use crate::pool::Db;

/// Seeds the default when no provider row exists, answering whether it did.
const INSERT_DEFAULT: &str = "INSERT INTO core.platform_provider_defaults \
     (provider, source_workspace_id, active, model, context_cap_tokens, created_at, updated_at) \
     VALUES ($1, $2::uuid, TRUE, $3, $4, $5, $5) \
     ON CONFLICT (provider) DO NOTHING \
     RETURNING provider";

/// Removes exactly the row this process inserted.
const DELETE_DEFAULT: &str = "DELETE FROM core.platform_provider_defaults \
     WHERE provider = $1 AND source_workspace_id = $2::uuid";

/// Which database a connection is on, so the drop deletes from that one.
const CURRENT_DATABASE: &str = "SELECT current_database()";

/// How long a new holder waits between looks while a delete is running.
const DELETE_POLL: Duration = Duration::from_millis(10);

/// The default a suite asks for.
#[derive(Debug, Clone, Copy)]
pub struct DefaultSeed<'a> {
    /// The provider the row is keyed by.
    pub provider: &'a str,
    /// The workspace whose vault holds the provider key.
    pub source_workspace_id: &'a str,
    /// The priced model the default names.
    pub model: &'a str,
    /// The context ceiling the default carries.
    pub context_cap_tokens: i32,
    /// The instant stamped on the row.
    pub created_at: i64,
}

/// The row this process inserted, and the database it lives in.
#[derive(Debug, Clone)]
struct Created {
    url: String,
    workspace: String,
}

/// One provider's holders in this process, and its row when this process made it.
#[derive(Debug, Default)]
struct State {
    holders: usize,
    created: Option<Created>,
    deleting: bool,
}

/// Per provider, because a suite that seeds a provider of its own must not
/// share a count — or a delete — with one that seeds the deployment's.
static STATE: Mutex<BTreeMap<String, State>> = Mutex::new(BTreeMap::new());

fn states() -> MutexGuard<'static, BTreeMap<String, State>> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A test's hold on one provider's platform default. Keep it bound until the
/// test ends, or drop it where the row must be gone.
#[derive(Debug)]
#[must_use = "the default is removed when the last hold drops, so bind it for the test's life"]
pub struct PlatformDefault {
    provider: String,
}

impl PlatformDefault {
    /// Seeds `seed` unless its provider already has a default, and holds it.
    ///
    /// # Panics
    /// When the insert fails — the fixture cannot give the test what it needs.
    pub async fn hold(database: &Db, seed: DefaultSeed<'_>) -> Self {
        let hold = Self::enter(seed.provider).await;
        let mut connection = acquired(database).await;
        let inserted: Option<String> = sqlx::query_scalar(INSERT_DEFAULT)
            .bind(seed.provider)
            .bind(seed.source_workspace_id)
            .bind(seed.model)
            .bind(seed.context_cap_tokens)
            .bind(seed.created_at)
            .fetch_optional(&mut *connection)
            .await
            .unwrap_or_else(|failure| panic!("the platform default seed must run: {failure}"));
        if inserted.is_some() {
            hold.record(&mut connection, seed.source_workspace_id).await;
        }
        hold
    }

    /// Holds a default the test created through the product rather than by
    /// insert, so it is removed even when the test fails before its cleanup.
    pub async fn adopt(database: &Db, provider: &str, source_workspace_id: &str) -> Self {
        let hold = Self::enter(provider).await;
        let mut connection = acquired(database).await;
        hold.record(&mut connection, source_workspace_id).await;
        hold
    }

    /// Notes that this process owns the row, and which database holds it.
    async fn record(&self, connection: &mut sqlx::PgConnection, workspace: &str) {
        let database: String = sqlx::query_scalar(CURRENT_DATABASE)
            .fetch_one(&mut *connection)
            .await
            .unwrap_or_else(|failure| panic!("the database must name itself: {failure}"));
        let url = super::TestDatabase {
            base_url: lane_url(),
            owned: Some(database),
        }
        .url();
        states().entry(self.provider.clone()).or_default().created = Some(Created {
            url,
            workspace: workspace.to_owned(),
        });
    }

    /// Counts a new holder, waiting out a delete of the same provider's row.
    async fn enter(provider: &str) -> Self {
        loop {
            {
                let mut states = states();
                let state = states.entry(provider.to_owned()).or_default();
                if !state.deleting {
                    state.holders += 1;
                    return Self {
                        provider: provider.to_owned(),
                    };
                }
            }
            tokio::time::sleep(DELETE_POLL).await;
        }
    }
}

impl Drop for PlatformDefault {
    fn drop(&mut self) {
        let created = {
            let mut states = states();
            let state = states.entry(self.provider.clone()).or_default();
            state.holders = state.holders.saturating_sub(1);
            if state.holders > 0 {
                return;
            }
            let Some(created) = state.created.take() else {
                return;
            };
            state.deleting = true;
            created
        };
        // A thread of its own, with a runtime of its own: a drop cannot await,
        // and blocking the test's runtime on its own connection would hang a
        // current-thread test.
        let provider = self.provider.clone();
        let removed = std::thread::spawn(move || remove(&provider, &created)).join();
        states().entry(self.provider.clone()).or_default().deleting = false;
        let failure = match removed {
            Ok(Ok(())) => return,
            Ok(Err(cause)) => cause.to_string(),
            Err(_) => "the removal thread panicked".to_owned(),
        };
        tracing::warn!(
            event = "test_platform_default_not_removed",
            error = %failure,
            "the platform default this run seeded could not be removed; reset the lane"
        );
    }
}

/// A connection on `database`, or the panic a fixture owes.
async fn acquired(database: &Db) -> sqlx::pool::PoolConnection<sqlx::Postgres> {
    database
        .acquire()
        .await
        .unwrap_or_else(|failure| panic!("the lane database must accept a connection: {failure}"))
}

/// The role the removal's own connection is reported under.
const REMOVAL_ROLE: &str = "test platform-default removal";

/// The operation a failed delete is reported under.
const REMOVAL_CONTEXT: &str = "removing the platform default a test seeded";

/// Deletes the row `created` names, on a connection of its own.
fn remove(provider: &str, created: &Created) -> crate::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|failure| panic!("the lane must start a removal runtime: {failure}"));
    runtime.block_on(async {
        let mut connection = sqlx::PgConnection::connect(&created.url)
            .await
            .map_err(|source| {
                crate::Error::new(crate::error::ErrorKind::DatastoreUnavailable {
                    role: REMOVAL_ROLE,
                    source,
                })
            })?;
        sqlx::query(DELETE_DEFAULT)
            .bind(provider)
            .bind(&created.workspace)
            .execute(&mut connection)
            .await
            .map_err(|source| crate::error::query(REMOVAL_CONTEXT, source))?;
        Ok(())
    })
}
