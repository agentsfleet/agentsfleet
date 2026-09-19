//! Who a proven credential belongs to — the read behind `GET /v1/users/me`.
//!
//! # Why this is its own module and not a method on the credential store
//!
//! [`crate::cli_credential::CliCredentials`] already resolves a subject to a
//! user row, so the method could have gone there. Its own doc comment is the
//! argument against: that store is "mint and revoke, and nothing else", and a
//! read whose whole job is answering a question about a PERSON is not about
//! their credentials. Two of the three credential classes that reach this read
//! are not command-line credentials at all.
//!
//! # Nothing here writes
//!
//! A live credential naming no local user is REFUSED, never provisioned on the
//! fly. That is [`crate::cli_credential`]'s rule and the reason is the same one:
//! minting a user row from a read path is how one identity ends up existing in
//! two places with different truths. This module holds one `SELECT` and no
//! statement that could insert one.

use afd_core::id::Uuid7;
use afd_db::Db;

use crate::sql::identity as sql;
use crate::{Result, error};

/// The context a datastore failure on this read reports under.
const CONTEXT_PROFILE: &str = "resolve caller profile";

/// The people a proven subject can be resolved to.
#[derive(Debug, Clone)]
pub struct Identities {
    database: Db,
}

impl Identities {
    /// A directory reading through `database`.
    #[must_use]
    pub const fn new(database: Db) -> Self {
        Self { database }
    }

    /// Answers the person a proven identity-provider subject names.
    ///
    /// # Errors
    /// Refuses a subject with no `core.users` row — a credential that
    /// authenticates and names nobody here, which an erased account and a
    /// credential minted against another directory both produce. Reports a
    /// datastore that would not answer, and a stored identifier that is not a
    /// version-7 Universally Unique Identifier (UUID).
    pub async fn profile(&self, subject: &str) -> Result<Profile> {
        let mut connection = self.database.acquire().await?;
        let row: Option<(String, String, Option<String>, String, String)> =
            sqlx::query_as(sql::SELECT_CALLER_PROFILE_BY_SUBJECT)
                .bind(subject)
                .fetch_optional(connection.as_mut())
                .await
                .map_err(error::query(CONTEXT_PROFILE))?;

        let (user, email, display_name, tenant, tenant_name) =
            row.ok_or_else(error::unknown_subject)?;
        Ok(Profile {
            user: Uuid7::parse(&user)?,
            email,
            display_name,
            tenant: Uuid7::parse(&tenant)?,
            tenant_name,
        })
    }
}

/// A person, as the account they signed up with describes them.
///
/// `display_name` is the one optional field, and it is optional because the
/// column is: an identity provider that sent an email and no name at signup
/// leaves it `NULL`, and inventing "Unknown" here would put a value in the
/// response that nobody typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// `core.users.id` — this person's row, not the provider's subject.
    pub user: Uuid7,
    /// The address the account was opened with.
    pub email: String,
    /// What they asked to be called, when they said.
    pub display_name: Option<String>,
    /// The tenant they act in, taken from the joined user row.
    ///
    /// Authoritative, where the copy stamped on a credential row at mint is
    /// provenance — the distinction [`crate::cli_credential::UserIdentity`]
    /// records for the same pair of columns.
    pub tenant: Uuid7,
    /// That tenant's name, which is what a person recognises.
    pub tenant_name: String,
}
