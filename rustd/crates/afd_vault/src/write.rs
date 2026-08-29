//! The two writes: claiming a name, and replacing a body.
//!
//! Both seal through the same [`Vault::seal`] and differ only in the statement
//! and in what zero affected rows MEANS — a name already taken on one, a name
//! never held on the other. That difference is the whole reason they are two
//! verbs rather than one with a flag.
//!
//! # No read precedes either write
//!
//! Not for speed. `PATCH {api_key}` used to load the stored body, merge one
//! hard-coded field, and re-store through an upsert — two autocommit statements
//! with nothing held between them, so a delete committing in the gap left the
//! upsert with no row to conflict against and it re-INSERTED the credential
//! that had just been removed. A single statement has no such gap. Replacement
//! is total for the same reason it is safe: a stored secret is never readable,
//! so a partial write is one the caller cannot reason about.

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_crypto::aad::Aad;
use afd_crypto::envelope::Envelope;

use crate::error::{ErrorKind, Result, query};
use crate::secret::{SecretBody, SecretName};
use crate::{Vault, sql};

/// The context a failed create reports under.
const CONTEXT_CREATE: &str = "claim a secret name";

/// The context a failed replace reports under.
const CONTEXT_REPLACE: &str = "replace a secret body";

impl Vault {
    /// Stores `body` under a name this workspace does not yet hold.
    ///
    /// # Errors
    /// Refuses a name the workspace already holds — nothing is written, and the
    /// decision is Postgres's, so two concurrent creates on one name resolve to
    /// one success and one refusal rather than to a silent overwrite. Reports a
    /// datastore that would not answer, an envelope that would not seal, and a
    /// host short of the entropy an identifier is minted from.
    pub async fn create(
        &self,
        workspace: &Uuid7,
        name: &SecretName,
        body: &SecretBody,
        now: UnixMillis,
    ) -> Result<()> {
        let envelope = self.seal(workspace, name, body)?;
        let id = self.mint_id(now)?;
        let projection = body.projection();

        let mut connection = self.directory.database.acquire().await?;
        let written = sqlx::query(sql::INSERT_SECRET_IF_ABSENT)
            .bind(id.as_str())
            .bind(workspace.as_str())
            .bind(name.as_str())
            .bind(envelope.wrapped_dek())
            .bind(envelope.dek_nonce().as_slice())
            .bind(envelope.dek_tag().as_slice())
            .bind(envelope.payload_nonce().as_slice())
            .bind(envelope.payload_ciphertext())
            .bind(envelope.payload_tag().as_slice())
            .bind(envelope.kek_version())
            .bind(now.as_millis())
            .bind(projection.kind.as_str())
            .bind(projection.provider.as_deref())
            .bind(projection.base_url.as_deref())
            .bind(projection.has_key)
            .execute(connection.as_mut())
            .await
            .map_err(query(CONTEXT_CREATE))?;

        // Zero rows is the `ON CONFLICT DO NOTHING` arm: somebody holds the
        // name and no ciphertext was written.
        if written.rows_affected() == 0 {
            return Err(ErrorKind::NameTaken.into());
        }
        let workspace_id = workspace.as_str();
        let secret_name = name.as_str();
        let secret_kind = projection.kind.as_str();
        tracing::debug!(
            workspace = workspace_id,
            name = secret_name,
            kind = secret_kind,
            event = "secret_created",
        );
        Ok(())
    }

    /// Replaces the whole body of a secret this workspace already holds.
    ///
    /// A field absent from `body` is absent from the stored secret afterwards.
    /// That is the point of the verb, and it is why no caller needs to read a
    /// secret back in order to change it.
    ///
    /// # Errors
    /// Refuses a name this workspace does not hold — the statement is an
    /// UPDATE, so it creates nothing and a replace racing a delete cannot
    /// resurrect the row. Reports a datastore that would not answer and an
    /// envelope that would not seal.
    pub async fn replace(
        &self,
        workspace: &Uuid7,
        name: &SecretName,
        body: &SecretBody,
        now: UnixMillis,
    ) -> Result<()> {
        let envelope = self.seal(workspace, name, body)?;
        let projection = body.projection();

        let mut connection = self.directory.database.acquire().await?;
        let written = sqlx::query(sql::UPDATE_SECRET)
            .bind(workspace.as_str())
            .bind(name.as_str())
            .bind(envelope.wrapped_dek())
            .bind(envelope.dek_nonce().as_slice())
            .bind(envelope.dek_tag().as_slice())
            .bind(envelope.payload_nonce().as_slice())
            .bind(envelope.payload_ciphertext())
            .bind(envelope.payload_tag().as_slice())
            .bind(envelope.kek_version())
            .bind(now.as_millis())
            .bind(projection.kind.as_str())
            .bind(projection.provider.as_deref())
            .bind(projection.base_url.as_deref())
            .bind(projection.has_key)
            .execute(connection.as_mut())
            .await
            .map_err(query(CONTEXT_REPLACE))?;

        // Zero rows means this workspace holds no such name. Nothing was
        // written and nothing was created.
        if written.rows_affected() == 0 {
            return Err(ErrorKind::NotFound.into());
        }
        let workspace_id = workspace.as_str();
        let secret_name = name.as_str();
        let secret_kind = projection.kind.as_str();
        tracing::debug!(
            workspace = workspace_id,
            name = secret_name,
            kind = secret_kind,
            event = "secret_replaced",
        );
        Ok(())
    }

    /// Seals `body` for the row at `(workspace, name)`.
    ///
    /// The associated data binds both, so a row lifted into another workspace
    /// or renamed fails its authentication tag instead of decrypting — the
    /// ciphertext columns are not portable on their own.
    fn seal(&self, workspace: &Uuid7, name: &SecretName, body: &SecretBody) -> Result<Envelope> {
        Ok(self.sealer.seal(
            &self.kek,
            &Aad::new(workspace.as_str(), name.as_str()),
            body.plaintext(),
        )?)
    }

    /// Draws a fresh row identifier.
    ///
    /// Minted from `now` rather than from a second clock read, so the row's
    /// identifier sorts beside the `created_at` written in the same statement.
    fn mint_id(&self, now: UnixMillis) -> Result<Uuid7> {
        let mut bytes = [0_u8; ENTROPY_LEN];
        self.entropy.fill(&mut bytes)?;
        Ok(Uuid7::encode(now, bytes)?)
    }
}
