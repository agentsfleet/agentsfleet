//! Opening a personal account from an identity provider's signup event.
//!
//! Five rows, one transaction, because four of five is not an account: a user
//! with no membership resolves to no workspace, and a tenant with no wallet
//! answers 500 on every billing read with no path back.
//!
//!   1. `core.tenants`          — one per person
//!   2. `core.users`            — the OIDC subject, unique
//!   3. `core.memberships`      — owner, linking the two
//!   4. `core.workspaces`       — a default, named by [`super::workspace::name`]
//!   5. `billing.tenant_wallet` — the starter grant
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

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_db::Db;
use afd_crypto::entropy::Entropy;

use crate::sql::signup as sql;
use crate::workspace::name;
use crate::{Result, error};

/// The context a failed statement is reported under.
const CONTEXT_EXISTING: &str = "read an existing account";
const CONTEXT_OPEN: &str = "open a personal account";

/// The tenant-level role a personal account's one member holds.
///
/// Bound as a parameter to both the membership insert and the lookup join, so
/// the word a signup writes and the word a later read matches on cannot drift.
/// Personal accounts have exactly one owner; teams are later work.
const OWNER_ROLE: &str = "owner";

/// Stamped into the workspace row so analytics can tell a bootstrapped
/// workspace from one a person created.
const BOOTSTRAP_ACTOR: &str = "signup_bootstrap";

/// The canonical nanos-per-USD factor the wallet column is denominated in.
pub const NANOS_PER_USD: i64 = 1_000_000_000;

/// The one-time starter balance a new tenant opens with, in nanos.
///
/// The only credit INFLOW this daemon has. Every other movement of
/// `balance_nanos` is a drain and happens in SQL, in the lease renew and settle
/// writable CTEs.
pub const STARTER_CREDIT_NANOS: i64 = 5 * NANOS_PER_USD;

/// What the starter grant is recorded as having come from.
const BOOTSTRAP_GRANT_SOURCE: &str = "bootstrap_starter_grant";

/// How many workspace names are tried before the attempt is abandoned.
///
/// A guard, not a budget. Uniqueness is per freshly-created tenant, which holds
/// no workspaces at all, so the first draw collides with nothing and the retry
/// exists only for two identical draws.
const GENERATED_ATTEMPTS: u32 = 3;

/// Who is being provisioned.
///
/// A struct rather than three positional `&str`s, which would be mutually
/// assignable: an address in the subject's place would compile and open an
/// account nobody can authenticate as.
#[derive(Debug, Clone, Copy)]
pub struct NewAccount<'a> {
    /// The identity provider's own subject, and the account's unique key.
    pub oidc_subject: &'a str,
    /// The primary address the provider reported.
    pub email: &'a str,
    /// What to call them, when the provider said.
    pub display_name: Option<&'a str>,
}

/// The account a signup resolved to, however it got there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bootstrapped {
    /// The person.
    pub user_id: String,
    /// Their tenant.
    pub tenant_id: String,
    /// Their default workspace.
    pub workspace_id: String,
    /// What that workspace is called.
    pub workspace_name: String,
    /// `true` on a fresh bootstrap, `false` on an idempotent replay.
    pub created: bool,
}

/// The tenant name a personal account is opened under.
///
/// The local part of the address, which is what a person recognises in a
/// workspace switcher. `None` for an address carrying no local part: that is a
/// malformed event rather than a person without a name, and a caller refuses it
/// exactly as it refuses an event carrying no address at all.
///
/// The Zig substitutes a fixed word here instead. That hides an invalid input
/// behind a tenant indistinguishable from any other, and validating at the
/// boundary is this port's rule rather than the Zig's (RULE PORT).
#[must_use]
pub fn personal_tenant_name(email: &str) -> Option<&str> {
    let local = email.split('@').next().unwrap_or_default();
    (!local.is_empty()).then_some(local)
}

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
        now: UnixMillis,
    ) -> Result<Bootstrapped> {
        if let Some(existing) = self.existing(account.oidc_subject).await? {
            return self.replay(existing, now).await;
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
                        self.replay(existing, now).await
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
        let row = sqlx::query_as::<_, (String, String, String, String)>(sql::SELECT_EXISTING)
            .bind(oidc_subject)
            .bind(OWNER_ROLE)
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_EXISTING))?;

        Ok(row.map(
            |(user_id, tenant_id, workspace_id, workspace_name)| Bootstrapped {
                user_id,
                tenant_id,
                workspace_id,
                workspace_name,
                created: false,
            },
        ))
    }

    /// Answers an already-provisioned account, healing its wallet on the way.
    async fn replay(&self, existing: Bootstrapped, now: UnixMillis) -> Result<Bootstrapped> {
        let mut connection = self.database.acquire().await?;
        let healed = sqlx::query(sql::INSERT_WALLET)
            .bind(&existing.tenant_id)
            .bind(STARTER_CREDIT_NANOS)
            .bind(BOOTSTRAP_GRANT_SOURCE)
            .bind(now.as_millis())
            .execute(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_EXISTING))?;

        if healed.rows_affected() > 0 {
            tracing::info!(
                event = "signup_replay_wallet_healed",
                "a replay restored a wallet row that had gone missing"
            );
        }
        tracing::info!(event = "signup_replay", "this subject already had an account");
        Ok(existing)
    }

    /// The five inserts, in one transaction.
    async fn open(
        &self,
        account: NewAccount<'_>,
        tenant_name: &str,
        now: UnixMillis,
    ) -> Result<Bootstrapped> {
        let tenant_id = self.mint_id(now)?;
        let user_id = self.mint_id(now)?;
        let membership_id = self.mint_id(now)?;
        let workspace_id = self.mint_id(now)?;

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
            .bind(now.as_millis())
            .execute(&mut *transaction)
            .await
            .map_err(&raise)?;

        sqlx::query(sql::INSERT_USER)
            .bind(user_id.as_str())
            .bind(tenant_id.as_str())
            .bind(account.oidc_subject)
            .bind(account.email)
            .bind(account.display_name)
            .bind(now.as_millis())
            .execute(&mut *transaction)
            .await
            .map_err(&raise)?;

        sqlx::query(sql::INSERT_MEMBERSHIP)
            .bind(membership_id.as_str())
            .bind(tenant_id.as_str())
            .bind(user_id.as_str())
            .bind(OWNER_ROLE)
            .bind(now.as_millis())
            .execute(&mut *transaction)
            .await
            .map_err(&raise)?;

        let workspace_name = self
            .name_a_workspace(&mut transaction, &tenant_id, &workspace_id, now)
            .await?;

        sqlx::query(sql::INSERT_WALLET)
            .bind(tenant_id.as_str())
            .bind(STARTER_CREDIT_NANOS)
            .bind(BOOTSTRAP_GRANT_SOURCE)
            .bind(now.as_millis())
            .execute(&mut *transaction)
            .await
            .map_err(&raise)?;

        transaction.commit().await.map_err(&raise)?;

        tracing::info!(event = "signup_bootstrapped", "a personal account was opened");
        Ok(Bootstrapped {
            user_id: user_id.as_str().to_owned(),
            tenant_id: tenant_id.as_str().to_owned(),
            workspace_id: workspace_id.as_str().to_owned(),
            workspace_name,
            created: true,
        })
    }

    /// Inserts the workspace under the first generated name that does not
    /// collide.
    ///
    /// The same retry `workspace::directory` runs for a caller who supplied no
    /// name, over a statement that yields instead of raising — see
    /// [`sql::INSERT_WORKSPACE_IF_FREE`] on why a transaction needs that.
    async fn name_a_workspace(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        tenant: &Uuid7,
        workspace: &Uuid7,
        now: UnixMillis,
    ) -> Result<String> {
        let raise = error::query(CONTEXT_OPEN);
        for _attempt in 0..GENERATED_ATTEMPTS {
            let candidate = name::generate(&self.entropy)?;
            let landed = sqlx::query(sql::INSERT_WORKSPACE_IF_FREE)
                .bind(workspace.as_str())
                .bind(tenant.as_str())
                .bind(&candidate)
                .bind(BOOTSTRAP_ACTOR)
                .bind(now.as_millis())
                .execute(&mut **transaction)
                .await
                .map_err(&raise)?;
            if landed.rows_affected() > 0 {
                return Ok(candidate);
            }
        }
        Err(error::workspace_name_exists())
    }

    /// A fresh identifier, stamped with this signup's instant.
    fn mint_id(&self, now: UnixMillis) -> Result<Uuid7> {
        let mut draw = [0_u8; ENTROPY_LEN];
        self.entropy.fill(&mut draw)?;
        Ok(Uuid7::encode(now, draw)?)
    }
}
