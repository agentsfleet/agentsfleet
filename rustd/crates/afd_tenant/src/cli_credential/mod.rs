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
//! The Zig original took a transaction-scoped advisory lock instead
//! (`pg_advisory_xact_lock(hashtextextended(user || ':' || machine, 0))`),
//! which works and costs two things: a Postgres-specific mechanism in the
//! domain layer, and a 64-bit hash of a concatenated pair, so two unrelated
//! users can collide onto one lock key and serialise against each other for no
//! reason. The retry needs neither, and the index it leans on is the one that
//! was deciding the outcome anyway.
//!
//! # Why the revoke and the insert are one transaction
//!
//! A re-login that fails must leave the operator holding the credential they
//! arrived with. Revoking first and inserting second is only safe if the two
//! commit together, and here they do — the transaction guard rolls back when it
//! is dropped, on every path including a `?` that returns early.
//!
//! That is worth stating because the Zig original could not rely on it. There,
//! the rollback is an `errdefer` that must be registered before the first
//! statement inside the transaction — registered later, a failure strands an
//! open transaction on a pooled connection — and it must call the driver's
//! `rollback()` rather than `exec("ROLLBACK")`, because `exec` short-circuits
//! once the connection is in FAIL state and would leave the session stuck in an
//! aborted transaction. Two ordering rules a reader has to know and a writer
//! has to remember. `sqlx::Transaction`'s `Drop` is the same guarantee with
//! nothing to remember, so this module states the intent and the type keeps it.
//!
//! # The digest is derived, never accepted
//!
//! [`CliCredentials::mint`] is the only writer and computes the digest from the
//! value it just drew. There is no path that takes a hash from a caller: if a
//! client could supply one, that digest would BE the credential and storing a
//! hash would protect nothing.

mod machine;

use afd_auth::credential::CredentialKind;
use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use sqlx::Acquire as _;

use crate::sql::cli_credential as sql;
use crate::{Result, error};
use afd_auth::minted::Minted;

pub use self::machine::MachineName;

/// The context a datastore failure on the mint path is reported under.
const CONTEXT_MINT: &str = "mint cli-credential";

/// The context the owner-scoped revoke reports under.
const CONTEXT_REVOKE: &str = "revoke cli-credential";

/// The context the subject lookup reports under.
const CONTEXT_SUBJECT: &str = "resolve subject user";

/// The Postgres error class for a violated unique index.
const UNIQUE_VIOLATION: &str = "23505";

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
        let row: Option<(String, String)> = sqlx::query_as(sql::SELECT_USER_IDENTITY_BY_SUBJECT)
            .bind(subject)
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_SUBJECT))?;

        let (id, tenant) = row.ok_or_else(error::cli_credential_unknown_subject)?;
        Ok(UserIdentity {
            id: Uuid7::parse(&id)?,
            tenant: Uuid7::parse(&tenant)?,
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
        let id = self.mint_id(now)?;

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

    /// Draws a fresh credential-row identifier.
    fn mint_id(&self, now: UnixMillis) -> Result<Uuid7> {
        let mut bytes = [0u8; ENTROPY_LEN];
        self.entropy.fill(&mut bytes)?;
        Ok(Uuid7::encode(now, bytes)?)
    }
}

/// Tells a lost race apart from a broken statement.
///
/// `23505` on this insert has exactly one cause: the partial unique index on
/// `(user_id, machine_name) WHERE revoked_at IS NULL` refused a second live row
/// for this machine. Everything else is a genuine fault.
fn classify_insert(source: sqlx::Error) -> crate::Error {
    let collided = source
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code == UNIQUE_VIOLATION);
    if collided {
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

/// The user row a command-line credential belongs to.
///
/// Both halves come from ONE read. The tenant is the joined user row's, which
/// is the authoritative one — the copy stamped on a credential row at mint is
/// provenance, never authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserIdentity {
    /// `core.users.id`, which the credential's foreign key points at.
    pub id: Uuid7,
    /// The tenant that user belongs to.
    pub tenant: Uuid7,
}

/// What minting one credential needs.
#[derive(Debug, Clone, Copy)]
pub struct MintRequest<'a> {
    /// Whose credential it is, as `core.users.id`.
    pub user: &'a Uuid7,
    /// The tenant that user belongs to.
    pub tenant: &'a Uuid7,
    /// The terminal's label, already parsed.
    pub machine: MachineName<'a>,
    /// The deployment answering this request.
    ///
    /// Never a value the caller supplied: a credential and the deployment that
    /// minted it are one fact, and a client-asserted host would let them
    /// disagree.
    pub deployment: &'a str,
    /// Where the mint was requested from, for the audit trail.
    pub from_address: &'a str,
}

/// A credential, and the one view of its plaintext that will ever exist.
///
/// No `Clone`, for [`crate::apikey::Revealed`]'s reason: a second copy of a
/// credential is a second thing to zero, and the one that gets missed is the
/// one that stays in the heap.
#[derive(Debug)]
pub struct Revealed {
    /// The credential row's identifier.
    pub id: Uuid7,
    /// The terminal's label.
    pub machine_name: String,
    /// The plaintext, which zeroes when this is dropped.
    pub credential: Minted,
    /// The deployment that minted it.
    pub deployment: String,
}

/// A credential that this call revoked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revoked {
    /// The credential row's identifier.
    pub id: Uuid7,
    /// When the row records it stopped working.
    pub revoked_at_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::{DISPLAY_HEX_LEN, display_prefix};
    use afd_auth::credential::CLI_CREDENTIAL_PREFIX;

    #[test]
    fn a_display_prefix_reveals_the_marker_and_eight_hex_characters() {
        let credential = format!("{CLI_CREDENTIAL_PREFIX}{}", "a".repeat(64));
        let shown = display_prefix(&credential);

        assert_eq!(
            shown.len(),
            CLI_CREDENTIAL_PREFIX.len() + DISPLAY_HEX_LEN,
            "the stored fragment is the marker plus eight characters"
        );
        assert!(
            credential.starts_with(shown),
            "the fragment must be a prefix of the value it identifies"
        );
        assert!(
            shown.len() < credential.len(),
            "a fragment as long as the credential would store the credential"
        );
    }

    #[test]
    fn a_value_shorter_than_the_fragment_is_returned_whole() {
        // Unreachable through `mint`, which always draws a full-length value.
        // Asserted anyway because the alternative implementation — a bare
        // slice — panics here rather than returning, and a panic on a
        // credential path is a denial of service reachable by a future caller.
        let short = "afc_ab";
        assert_eq!(
            display_prefix(short),
            short,
            "a short value is returned rather than sliced out of bounds"
        );
    }
}
