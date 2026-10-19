//! Opening a personal account from an identity provider's signup event.
//!
//! Five rows, one transaction, because four of five is not an account: a user
//! with no membership resolves to no workspace, and a tenant with no wallet
//! answers 500 on every billing read with no path back.
//!
//!   1. `identity.tenants`     — one per person
//!   2. `identity.users`       — the OIDC subject, unique
//!   3. `identity.memberships` — owner, linking the two
//!   4. `identity.workspaces`  — a default, named by [`super::workspace::name`]
//!   5. `billing.wallets`      — the starter grant
//!
//! # Why this lives in the tenant crate
//!
//! Every row it writes is this crate's: the tenant, the person, the membership
//! and the workspace. It also needs the workspace name generator, and
//! `workspace::name`'s own module note already anticipated this caller —
//! *"Signup bootstrap, meanwhile, generates one"*. A separate crate for it
//! would exist mainly to depend on this one.
//!
//! # Idempotent on the OIDC subject, and why that is the whole design
//!
//! An identity provider retries. A second `user.created` for a subject already
//! provisioned must answer exactly as the first did, so this reads before it
//! writes and answers [`Bootstrapped::created`] `false` on the replay.
//!
//! The pre-read is not enough on its own and is not meant to be. Two concurrent
//! deliveries can both pass it, because it runs before the transaction opens.
//! The first commits; the second trips the unique index on `oidc_subject`,
//! re-reads what the winner committed, and answers as a replay. The index is
//! the arbiter and the pre-read is only an optimisation — which is the right
//! way round, since an index holds under concurrency and a read never can.
//!
//! # The wallet is healed on replay, not merely skipped
//!
//! Only the create path writes the wallet, so a tenant that lost the row — a
//! bootstrap from before the grant existed, a restore — would 500 forever. The
//! replay path re-runs the insert, which is `ON CONFLICT DO NOTHING`, so a
//! healthy wallet including a spent-down balance is untouched and a missing one
//! comes back.

use afd_core::id::Uuid7;
use afd_core::money::NANOS_PER_USD;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_observability::producers;
use jiff::Timestamp;

use crate::sql::signup as sql;
use crate::workspace::access::ROLE_OWNER;
use crate::workspace::name;
use crate::{Result, error};

/// The context a failed statement is reported under.
const CONTEXT_EXISTING: &str = "read an existing account";
const CONTEXT_OPEN: &str = "open a personal account";

/// The one-time starter balance a new tenant opens with, in nanos.
///
/// The only credit INFLOW this daemon has. Every other movement of
/// `balance_nanos` is a drain and happens in SQL, in the lease renew and settle
/// writable CTEs.
pub const STARTER_CREDIT_NANOS: i64 = 5 * NANOS_PER_USD;

mod account;

pub use self::account::{Bootstrapped, NewAccount, personal_tenant_name};

/// Opening personal accounts, over one pool.
///
/// Cheap to clone: both members are handles.
#[derive(Debug, Clone)]
pub struct Signups {
    /// Where the five rows are written.
    database: Db,
    /// Where the four identifiers and the workspace name are drawn from.
    entropy: Entropy,
}

impl Signups {
    /// Binds provisioning to a pool and a random source.
    #[must_use]
    pub const fn new(database: Db, entropy: Entropy) -> Self {
        Self { database, entropy }
    }

    /// Resolves a signup event to an account, opening one if there is none.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a statement that failed, and
    /// an entropy source that would not answer. A replay is NOT an error — see
    /// the module note.
    pub async fn bootstrap(
        &self,
        account: NewAccount<'_>,
        tenant_name: &str,
        now: Timestamp,
    ) -> Result<Bootstrapped> {
        if let Some(existing) = self.existing(account.oidc_subject).await? {
            return self.replay(existing).await;
        }

        match self.open(account, tenant_name, now).await {
            Ok(opened) => Ok(opened),
            Err(raised) => {
                // The pre-read ran outside the transaction, so a concurrent
                // delivery may have committed since. Ask again: if the account
                // is there now, the index did its job and this is a replay.
                match self.existing(account.oidc_subject).await? {
                    Some(existing) => {
                        tracing::info!(
                            event = "signup_replay_after_race",
                            "a concurrent delivery opened this account first"
                        );
                        self.replay(existing).await
                    }
                    // Nothing committed, so the failure was real. The original
                    // error is returned rather than a fresh one, keeping the
                    // `source()` chain that names the statement.
                    None => Err(raised),
                }
            }
        }
    }

    /// The account this subject already owns, if it owns one.
    async fn existing(&self, oidc_subject: &str) -> Result<Option<Bootstrapped>> {
        let mut connection = self.database.acquire().await?;
        let row: Option<ExistingRow> = sqlx::query_as(sql::SELECT_EXISTING)
            .bind(oidc_subject)
            .bind(ROLE_OWNER)
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_EXISTING))?;

        Ok(row.map(|row| Bootstrapped {
            user_id: row.user_id,
            tenant_id: row.tenant_id,
            workspace_id: row.workspace_id,
            workspace_name: row.workspace_name,
            created: false,
        }))
    }

    /// Answers an already-provisioned account, healing its wallet on the way.
    async fn replay(&self, existing: Bootstrapped) -> Result<Bootstrapped> {
        let mut connection = self.database.acquire().await?;
        let healed = sqlx::query(sql::INSERT_WALLET)
            .bind(&existing.tenant_id)
            .bind(STARTER_CREDIT_NANOS)
            .execute(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_EXISTING))?;

        if healed.rows_affected() > 0 {
            tracing::info!(
                event = "signup_replay_wallet_healed",
                "a replay restored a wallet row that had gone missing"
            );
        }
        producers::fleet::signup_replayed();
        tracing::info!(
            event = "signup_replay",
            "this subject already had an account"
        );
        Ok(existing)
    }

    /// The five inserts, in one transaction.
    async fn open(
        &self,
        account: NewAccount<'_>,
        tenant_name: &str,
        now: Timestamp,
    ) -> Result<Bootstrapped> {
        let tenant_id = self.entropy.uuid7(now)?;
        let user_id = self.entropy.uuid7(now)?;
        let membership_id = self.entropy.uuid7(now)?;
        let workspace_id = self.entropy.uuid7(now)?;

        // `?` on every statement rolls back: a sqlx `Transaction` commits only
        // on an explicit `commit()` and rolls back when dropped, so an early
        // return cannot leave a half-opened account behind.
        let raise = error::query(CONTEXT_OPEN);
        let mut connection = self.database.acquire().await?;
        let mut transaction = sqlx::Acquire::begin(&mut *connection)
            .await
            .map_err(&raise)?;

        sqlx::query(sql::INSERT_TENANT)
            .bind(tenant_id.as_str())
            .bind(tenant_name)
            .execute(&mut *transaction)
            .await
            .map_err(&raise)?;

        Self::insert_owner(
            &mut transaction,
            &account,
            &tenant_id,
            &user_id,
            &membership_id,
        )
        .await?;

        let workspace_name = self
            .name_a_workspace(&mut transaction, &tenant_id, &workspace_id)
            .await?;

        sqlx::query(sql::INSERT_WALLET)
            .bind(tenant_id.as_str())
            .bind(STARTER_CREDIT_NANOS)
            .execute(&mut *transaction)
            .await
            .map_err(&raise)?;

        transaction.commit().await.map_err(&raise)?;

        producers::fleet::signup_bootstrapped();
        tracing::info!(
            event = "signup_bootstrapped",
            "a personal account was opened"
        );
        Ok(Bootstrapped {
            user_id: user_id.as_str().to_owned(),
            tenant_id: tenant_id.as_str().to_owned(),
            workspace_id: workspace_id.as_str().to_owned(),
            workspace_name,
            created: true,
        })
    }

    /// The person and their owner membership of the tenant just inserted,
    /// inside the opening transaction — the second and third of its inserts.
    async fn insert_owner(
        transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        account: &NewAccount<'_>,
        tenant_id: &Uuid7,
        user_id: &Uuid7,
        membership_id: &Uuid7,
    ) -> Result<()> {
        let raise = error::query(CONTEXT_OPEN);
        sqlx::query(sql::INSERT_USER)
            .bind(user_id.as_str())
            .bind(tenant_id.as_str())
            .bind(account.oidc_subject)
            .bind(account.email)
            .bind(account.display_name)
            .execute(&mut **transaction)
            .await
            .map_err(&raise)?;

        sqlx::query(sql::INSERT_MEMBERSHIP)
            .bind(membership_id.as_str())
            .bind(tenant_id.as_str())
            .bind(user_id.as_str())
            .bind(ROLE_OWNER)
            .execute(&mut **transaction)
            .await
            .map_err(&raise)?;
        Ok(())
    }

    /// Inserts the workspace under one generated name.
    ///
    /// One draw, no retry: names are unique per tenant, and the tenant was
    /// opened by this same transaction and holds no workspace yet, so the
    /// first name collides with nothing. The insert is
    /// [`crate::sql::workspace::INSERT_WORKSPACE`], which raises on a
    /// collision rather than yielding, because there is no second draw that a
    /// yielded row would feed.
    async fn name_a_workspace(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        tenant: &Uuid7,
        workspace: &Uuid7,
    ) -> Result<String> {
        let name = name::generate(&self.entropy)?;
        sqlx::query(crate::sql::workspace::INSERT_WORKSPACE)
            .bind(workspace.as_str())
            .bind(tenant.as_str())
            .bind(&name)
            .execute(&mut **transaction)
            .await
            .map_err(error::query(CONTEXT_OPEN))?;
        Ok(name)
    }
}

/// The account a subject already owns, by the names the statement aliases.
#[derive(Debug, sqlx::FromRow)]
struct ExistingRow {
    user_id: String,
    tenant_id: String,
    workspace_id: String,
    workspace_name: String,
}
