//! Command-line credentials: the `afc_` value `agentsfleet login` mints.
//!
//! # One live credential per machine, held by the datastore
//!
//! A partial unique index on `(user_id, machine_name) WHERE revoked_at IS NULL`
//! is what makes two live credentials for one terminal unrepresentable — not
//! discipline in this module. [`CliCredentials::mint`] revokes this machine's
//! live row before inserting its replacement, and skipping that step does not
//! produce two rows: it produces a failed insert.
//!
//! # Two simultaneous logins from one machine
//!
//! The partial unique index is the arbiter, and a loser is retried rather than
//! reported. Both callers revoke nothing (there is no live row on a first
//! login) and both insert; one wins and the other comes back `23505`. That is
//! the index doing its job, so the answer is to run the loser's transaction
//! again — its revoke now finds the winner's row and its insert succeeds. Last
//! login wins, which is what "one live credential per machine" means.
//!
//! A transaction-scoped advisory lock
//! (`pg_advisory_xact_lock(hashtextextended(user || ':' || machine, 0))`)
//! would also work, and costs two things: a Postgres-specific mechanism in the
//! domain layer, and a 64-bit hash of a concatenated pair, so two unrelated
//! users can collide onto one lock key and serialise against each other for no
//! reason. The retry needs neither, and the index it leans on is the one that
//! decides the outcome anyway.
//!
//! # Why the revoke and the insert are one transaction
//!
//! A re-login that fails must leave the operator holding the credential they
//! arrived with. Revoking first and inserting second is only safe if the two
//! commit together, and here they do — the transaction guard rolls back when it
//! is dropped, on every path including a `?` that returns early. A hand-placed
//! rollback would be an ordering rule every writer has to remember;
//! `sqlx::Transaction`'s `Drop` makes it a property of the type, so this
//! module states the intent and the type keeps it.
//!
//! # The digest is derived, never accepted
//!
//! [`CliCredentials::mint`] is the only writer and computes the digest from the
//! value it just drew. There is no path that takes a hash from a caller: if a
//! client could supply one, that digest would BE the credential and storing a
//! hash would protect nothing.

mod machine;
mod record;

use afd_auth::credential::CredentialKind;
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::constraint::violates_unique;
use sqlx::Acquire as _;

use crate::sql::cli_credential as sql;
use crate::sql::{COLUMN_ID, COLUMN_TENANT_ID};
use crate::{Result, error, stored};
use afd_auth::minted::Minted;

pub use self::machine::MachineName;
pub use self::record::{MintRequest, Revealed, Revoked, UserIdentity};

/// The context a datastore failure on the mint path is reported under.
const CONTEXT_MINT: &str = "mint cli-credential";

/// The context the owner-scoped revoke reports under.
const CONTEXT_REVOKE: &str = "revoke cli-credential";

/// The context the subject lookup reports under.
const CONTEXT_SUBJECT: &str = "resolve subject user";

/// The table a malformed user row is reported by.
const TABLE_USERS: &str = "core.users";

/// The partial index that holds a machine to one live credential.
///
/// Must equal the name in `schema/250_cli_credentials.sql`: a hash or
/// primary-key collision is unique too, and no re-login resolves it.
const MACHINE_CONSTRAINT: &str = "uq_cli_credentials_user_machine_live";

/// Leading hex characters kept for display beside a credential.
///
/// Eight of sixty-four leaves 224 bits unrevealed, so a stored display prefix
/// narrows an offline search by nothing that matters. It exists so an operator
/// can tell two credentials apart in a list without either being readable.
const DISPLAY_HEX_LEN: usize = 8;

/// A person's command-line credentials.
#[derive(Debug, Clone)]
pub struct CliCredentials {
    database: Db,
    entropy: Entropy,
}

impl CliCredentials {
    /// A store reading and writing through `database`.
    #[must_use]
    pub const fn new(database: Db, entropy: Entropy) -> Self {
        Self { database, entropy }
    }

    /// Resolves an authenticated subject to the user row these verbs write against.
    ///
    /// A live token for a subject with no local row is REFUSED rather than
    /// provisioned on the fly: minting a user row from an authenticate path is
    /// how one identity ends up existing in two places with different truths.
    ///
    /// # Errors
    /// Refuses a subject with no user row. Reports a datastore that would not
    /// answer.
    pub async fn user_of(&self, subject: &str) -> Result<UserIdentity> {
        let mut connection = self.database.acquire().await?;
        let row: Option<(String, String, String, Option<String>, String)> =
            sqlx::query_as(sql::SELECT_USER_IDENTITY_BY_SUBJECT)
                .bind(subject)
                .fetch_optional(connection.as_mut())
                .await
                .map_err(error::query(CONTEXT_SUBJECT))?;

        let (id, tenant, email, display_name, tenant_name) =
            row.ok_or_else(error::unknown_subject)?;
        Ok(UserIdentity {
            id: stored::uuid(TABLE_USERS, COLUMN_ID, &id)?,
            tenant: stored::uuid(TABLE_USERS, COLUMN_TENANT_ID, &tenant)?,
            email,
            display_name,
            tenant_name,
        })
    }

    /// Mints this machine's credential, revoking whatever it left behind.
    ///
    /// The two writes are one transaction, so a failed re-login leaves the
    /// operator holding the credential they arrived with.
    ///
    /// # Errors
    /// Reports a host that cannot draw entropy and a datastore that would not
    /// answer.
    pub async fn mint(&self, request: &MintRequest<'_>, now: UnixMillis) -> Result<Revealed> {
        match self.try_mint(request, now).await {
            // The index refused a second live row for this machine, which means
            // somebody else's login committed between our revoke and our
            // insert. Their row is live now, so a second attempt revokes it and
            // takes its place. Once, not in a loop: a second collision would
            // need a third simultaneous login on one machine in the width of
            // one transaction, and retrying forever on a condition that cannot
            // clear is how a mint path becomes a spin.
            Err(error) if error.is_machine_collision() => self.try_mint(request, now).await,
            outcome => outcome,
        }
    }

    /// One attempt at the mint, collision and all.
    async fn try_mint(&self, request: &MintRequest<'_>, now: UnixMillis) -> Result<Revealed> {
        // Both are drawn before the transaction opens. Neither touches the
        // datastore, and holding a transaction open across them would widen the
        // window on this write path for nothing.
        let credential = Minted::draw(CredentialKind::CliCredential, &self.entropy)?;
        let id = self.entropy.uuid7(now)?;

        let mut connection = self.database.acquire().await?;
        // Dropped without a commit — on a `?` below, or on a panic — this rolls
        // back. There is no reset to forget and no path that leaves the machine
        // revoked without its replacement written.
        let mut transaction = connection
            .begin()
            .await
            .map_err(error::query(CONTEXT_MINT))?;

        // Zero rows is a first login, not a failure: there is nothing to
        // revoke, and the insert below is the whole of the work.
        sqlx::query(sql::REVOKE_CLI_CREDENTIAL_FOR_MACHINE)
            .bind(request.user.as_str())
            .bind(request.machine.as_str())
            .bind(now.as_millis())
            .execute(&mut *transaction)
            .await
            .map_err(error::query(CONTEXT_MINT))?;

        // The one statement whose failure can be a RACE rather than a fault, so
        // it is the one classified rather than lifted.
        sqlx::query(sql::INSERT_CLI_CREDENTIAL)
            .bind(id.as_str())
            .bind(request.user.as_str())
            .bind(request.tenant.as_str())
            .bind(request.machine.as_str())
            .bind(credential.digest().as_str())
            .bind(display_prefix(credential.expose()))
            .bind(request.deployment)
            .bind(request.from_address)
            .bind(now.as_millis())
            .execute(&mut *transaction)
            .await
            .map_err(classify_insert)?;

        transaction
            .commit()
            .await
            .map_err(error::query(CONTEXT_MINT))?;

        // Attribution is a mint-time fact: recorded once, here, and never
        // written again on the authenticate path. The credential itself is
        // absent from this line and from every other emitted surface — the
        // hoisted bindings are values a log may carry, and `Minted`'s `Debug`
        // renders a length and the word redacted rather than the token.
        let credential_id = id.as_str();
        let machine_name = request.machine.as_str();
        let deployment = request.deployment;
        tracing::info!(
            credential_id,
            machine_name,
            deployment,
            event = "credential_minted"
        );

        Ok(Revealed {
            id,
            machine_name: request.machine.as_str().to_owned(),
            credential,
            deployment: request.deployment.to_owned(),
        })
    }

    /// Revokes one of this user's credentials by identifier.
    ///
    /// # Errors
    /// Refuses an id naming no LIVE credential this user holds — which is one
    /// answer for three situations, because telling them apart would confirm
    /// another person's credential to whoever guessed its identifier. Reports a
    /// datastore that would not answer.
    pub async fn revoke(
        &self,
        user: &Uuid7,
        credential: &Uuid7,
        now: UnixMillis,
    ) -> Result<Revoked> {
        let mut connection = self.database.acquire().await?;
        let affected = sqlx::query(sql::REVOKE_CLI_CREDENTIAL_BY_ID)
            .bind(credential.as_str())
            .bind(user.as_str())
            .bind(now.as_millis())
            .execute(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_REVOKE))?
            .rows_affected();

        if affected == 0 {
            return Err(error::cli_credential_not_found());
        }

        let credential_id = credential.as_str();
        tracing::info!(credential_id, event = "credential_revoked");
        Ok(Revoked {
            id: credential.clone(),
            revoked_at_ms: now.as_millis(),
        })
    }
}

/// Tells a lost race apart from a broken statement.
///
/// A violation of [`MACHINE_CONSTRAINT`], the partial unique index on
/// `(user_id, machine_name) WHERE revoked_at IS NULL`, is a second live row
/// for this machine: a lost race the mint retries. Everything else, another
/// unique index included, is a genuine fault.
fn classify_insert(source: sqlx::Error) -> crate::Error {
    if violates_unique(&source, MACHINE_CONSTRAINT) {
        error::cli_credential_machine_collision()
    } else {
        error::query(CONTEXT_MINT)(source)
    }
}

/// The non-secret fragment stored beside the digest.
///
/// Borrows rather than allocates, and is short-safe: a value shorter than the
/// display length is returned whole rather than sliced, so this cannot panic on
/// a boundary. Every credential this module draws is full length, which makes
/// the guard unreachable in practice and correct anyway.
fn display_prefix(credential: &str) -> &str {
    let shown = CredentialKind::CliCredential
        .prefix()
        .map_or(0, str::len)
        .saturating_add(DISPLAY_HEX_LEN);
    credential.get(..shown).unwrap_or(credential)
}

#[cfg(test)]
mod tests;
