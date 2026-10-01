//! The accounts a caller holds, which is what the workspace list walks.
//!
//! A person's own account, plus every account an accepted invite made them a
//! member of. A claim-bound credential holds its own account and nothing else,
//! for the reason the access check gives it nothing else: an api-key or a
//! terminal credential acts for the account it was minted in.

use afd_auth::principal::{Person, Principal};
use afd_core::id::Uuid7;
use sqlx::Row as _;
use sqlx::postgres::PgRow;

use super::access::{ROLE_OWNER, Role};
use super::{Workspaces, held_row, reads_user_row};
use crate::sql::workspace as sql;
use crate::{Result, error, stored};

/// The context a failed account read reports under.
const CONTEXT_ACCOUNTS: &str = "list held accounts";

/// One account a caller holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// The account's tenant.
    pub tenant: Uuid7,
    /// The caller's role in it.
    pub role: Role,
    /// What a person calls the account: its owner's display name, or the
    /// account's own name when the owner has none.
    pub owner_name: String,
}

/// Every account a caller holds, and which one is their own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accounts {
    /// The caller's own account, the one they act in when nothing else says.
    pub home: Uuid7,
    /// Each account held, the caller's own included when its row exists.
    pub held: Vec<Account>,
}

impl Accounts {
    /// The tenants to walk, in the order they were read, borrowed for the
    /// page statement's array bind.
    #[must_use]
    pub fn tenants(&self) -> Vec<&str> {
        self.held
            .iter()
            .map(|account| account.tenant.as_str())
            .collect()
    }

    /// The held account `tenant` names, when it is one of them.
    #[must_use]
    pub fn get(&self, tenant: &Uuid7) -> Option<&Account> {
        self.held.iter().find(|account| account.tenant == *tenant)
    }
}

impl Workspaces {
    /// The accounts `principal` holds, with its role in each.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a stored value this
    /// build cannot read. A runner holds no account and answers `Ok(None)`.
    pub async fn accounts_of(&self, principal: &Principal) -> Result<Option<Accounts>> {
        let Some(person) = principal.person() else {
            return Ok(None);
        };
        if reads_user_row(person)
            && let Some(accounts) = self.subject_accounts(person).await?
        {
            return Ok(Some(accounts));
        }
        // A claim-bound credential, or a session whose subject has no user
        // row: the claim stands, the fallback `tenant_of` also takes.
        self.claimed_account(person).await.map(Some)
    }

    /// Every account a signed-in person holds, or `None` with no user row.
    async fn subject_accounts(&self, person: &Person) -> Result<Option<Accounts>> {
        let mut connection = self.database.acquire().await?;
        let unreadable = error::query(CONTEXT_ACCOUNTS);
        let rows = sqlx::query(sql::SELECT_SUBJECT_ACCOUNTS)
            .bind(person.subject().as_str())
            .bind(ROLE_OWNER)
            .fetch_all(connection.as_mut())
            .await
            .map_err(&unreadable)?;
        let Some(first) = rows.first() else {
            return Ok(None);
        };
        let home: String = first.try_get("home_tenant_id").map_err(&unreadable)?;
        let home = stored::tenant(&home)?;
        let held = rows.iter().map(account).collect::<Result<_>>()?;
        Ok(Some(Accounts { home, held }))
    }

    /// The one account a claim names, held as its owner.
    async fn claimed_account(&self, person: &Person) -> Result<Accounts> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::SELECT_TENANT_ACCOUNT)
            .bind(person.tenant().as_str())
            .bind(ROLE_OWNER)
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_ACCOUNTS))?;
        let held = rows.iter().map(account).collect::<Result<_>>()?;
        Ok(Accounts {
            home: person.tenant().clone(),
            held,
        })
    }
}

/// One held account from its row, held as the access check holds it.
fn account(row: &PgRow) -> Result<Account> {
    let (tenant, role) = held_row(row, CONTEXT_ACCOUNTS)?;
    Ok(Account {
        tenant,
        role,
        owner_name: row
            .try_get(crate::sql::COLUMN_OWNER_NAME)
            .map_err(error::query(CONTEXT_ACCOUNTS))?,
    })
}
