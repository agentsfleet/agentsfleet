//! One checkpoint key per workspace: a row in that workspace's own vault, written by the daemon alone.
//!
//! The key is used as it is stored. It seals every file chunk and the turn
//! history root, and it keys each chunk's content identity. No table holds a
//! copy, and no other key is derived from it.

use af_core::{clock, error_code, id::Uuid7};
use af_crypto::{
    checkpoint::{Aes256Gcm, ChunkCipher, Plain, content_identity},
    entropy::Entropy,
    secret::{SecretBytes, SecretObject, SecretString},
};
use af_wire::durable_task::{CheckpointCipher, CheckpointPlaintext};
use afd_vault::{
    CHECKPOINT_KEY_NAME, SecretBody, SecretName, Vault,
    bag::{Bag, BagFault, secret},
};
use base64::{Engine as _, prelude::BASE64_STANDARD};
use sqlx::{PgConnection, Postgres, Transaction};
use zeroize::Zeroizing;

use crate::{Result, durable_task::DurableTaskRefusal};

/// The one field of the key's vault document: the key as 64 hexadecimal digits.
const FIELD: &str = "key";
const EVENT_CREATED: &str = "checkpoint_secret_created";
const EVENT_UNUSABLE: &str = "checkpoint_key_unusable";
const EVENT_PLAIN: &str = "checkpoint_cipher_plain";

/// The implementation a recorded cipher names.
#[must_use]
pub fn chunk_cipher(cipher: CheckpointCipher) -> &'static dyn ChunkCipher {
    match cipher {
        CheckpointCipher::Aes256Gcm => &Aes256Gcm,
        CheckpointCipher::Plain => &Plain,
    }
}

/// A recorded cipher spelling as the cipher it names.
pub(crate) fn recorded(cipher: &str) -> Result<CheckpointCipher> {
    cipher
        .parse()
        .map_err(|_unknown| DurableTaskRefusal::CheckpointIncompatible.into())
}

#[derive(serde::Deserialize)]
pub(crate) struct Stored {
    key: Option<SecretString>,
}

/// A workspace's checkpoint key, wiped when dropped.
pub(crate) struct WorkspaceKey(SecretBytes);

impl Bag for WorkspaceKey {
    type Stored = Stored;

    fn from_stored(stored: Stored) -> std::result::Result<Self, BagFault> {
        let text = secret(stored.key, FIELD)?;
        let mut bytes = Zeroizing::new([0_u8; af_crypto::KEY_LEN]);
        hex::decode_to_slice(text.expose(), bytes.as_mut_slice())
            .map_err(|_invalid| BagFault::Unparsed(FIELD))?;
        Ok(Self(SecretBytes::new(bytes.to_vec())))
    }
}

impl WorkspaceKey {
    pub(crate) const fn bytes(&self) -> &SecretBytes {
        &self.0
    }

    /// A chunk's content identity under this workspace's key.
    pub(crate) fn content(&self, descriptor: &CheckpointPlaintext, format: i32) -> String {
        content_identity(&self.0, format, &descriptor.sha256, descriptor.size)
    }

    /// The key as an authorization frame carries it to the supervisor.
    pub(crate) fn wire(&self) -> String {
        BASE64_STANDARD.encode(self.0.expose())
    }
}

/// What one retirement removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetiredCheckpoints {
    /// Checkpoint generations deleted, pending plans among them.
    pub generations: u64,
    /// Stored objects whose bucket delete is now owed.
    pub objects: u64,
}

/// What a restore finds where a workspace's key should be.
pub(crate) enum Held {
    Usable(WorkspaceKey),
    /// The row is missing, will not open, or holds no key.
    Unusable,
}

/// The deployment's checkpoint cipher and the vault every workspace's key lives in.
#[derive(Clone)]
pub struct CheckpointKeys {
    database: afd_db::Db,
    vault: Vault,
    entropy: Entropy,
    cipher: CheckpointCipher,
}

impl std::fmt::Debug for CheckpointKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckpointKeys")
            .field("cipher", &self.cipher)
            .finish_non_exhaustive()
    }
}

impl CheckpointKeys {
    /// Built once at boot from `[checkpoint] cipher`, and says so when the
    /// deployment seals nothing.
    #[must_use]
    pub fn new(database: afd_db::Db, vault: Vault, cipher: CheckpointCipher) -> Self {
        if cipher == CheckpointCipher::Plain {
            tracing::warn!(
                event = EVENT_PLAIN,
                "checkpoints are stored without encryption"
            );
        }
        Self {
            database,
            vault,
            entropy: Entropy::new(),
            cipher,
        }
    }

    /// Keys over `database` under `kek`, for a suite that builds its own plane.
    #[cfg(any(test, feature = "test-util"))]
    #[must_use]
    pub fn over(
        database: afd_db::Db,
        kek: std::sync::Arc<af_crypto::secret::Kek>,
        cipher: CheckpointCipher,
    ) -> Self {
        let vault = Vault::new(database.clone(), kek, Entropy::new());
        Self::new(database, vault, cipher)
    }

    /// The key a save in `workspace` seals under, through a save's own path:
    /// read in one transaction, created there when absent, and committed.
    ///
    /// # Errors
    /// As a save's registration: a datastore or vault failure, or a key row
    /// that exists and cannot be used.
    #[cfg(any(test, feature = "test-util"))]
    pub async fn save_key(&self, workspace: &Uuid7) -> Result<SecretBytes> {
        let mut connection = self.database.acquire().await?;
        let mut tx = sqlx::Connection::begin(&mut *connection)
            .await
            .map_err(crate::error::query("begin a first save's key read"))?;
        let key = self.for_save(&mut tx, workspace).await?;
        tx.commit()
            .await
            .map_err(crate::error::query("commit a first save's key"))?;
        Ok(SecretBytes::new(key.bytes().expose().to_vec()))
    }

    /// The cipher new checkpoints are sealed under.
    #[must_use]
    pub const fn cipher(&self) -> CheckpointCipher {
        self.cipher
    }

    pub(super) const fn vault(&self) -> &Vault {
        &self.vault
    }

    /// A random 96-bit nonce: every object and root of a workspace is sealed
    /// under one key, so none may repeat.
    pub(crate) fn nonce(&self) -> Result<[u8; af_crypto::NONCE_LEN]> {
        let mut nonce = [0_u8; af_crypto::NONCE_LEN];
        self.entropy.fill(&mut nonce)?;
        Ok(nonce)
    }

    /// The key a save seals under, created by the workspace's first save
    /// inside that save's transaction.
    ///
    /// The vault refuses a name the workspace holds, so of two first saves one
    /// creates the row and the other loads it.
    pub(crate) async fn for_save(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        workspace: &Uuid7,
    ) -> Result<WorkspaceKey> {
        match self.read(tx, workspace).await? {
            Ok(key) => return Ok(key),
            Err(BagFault::Absent) => {}
            Err(fault) => return Err(unusable(workspace, fault.name())),
        }
        let (name, body) = self.fresh()?;
        let created = self
            .vault
            .create_in(tx, workspace, &name, &body, clock::now())
            .await;
        match created {
            // No fleet declares this name, so the write has nobody to announce to.
            Ok(_nobody) => tracing::info!(event = EVENT_CREATED, workspace_id = %workspace),
            Err(taken) if taken.code() == error_code::SECRET_NAME_TAKEN => {}
            Err(failure) => return Err(failure.into()),
        }
        self.read(tx, workspace)
            .await?
            .map_err(|fault| unusable(workspace, fault.name()))
    }

    /// The key a restore opens under; never created here.
    pub(crate) async fn for_restore(
        &self,
        connection: &mut PgConnection,
        workspace: &Uuid7,
    ) -> Result<Held> {
        let fault = match self.read(connection, workspace).await {
            Ok(Ok(key)) => return Ok(Held::Usable(key)),
            Ok(Err(fault)) => fault.name(),
            Err(sealed) if sealed.code() == error_code::INTERNAL_OPERATION_FAILED => "unopenable",
            Err(failure) => return Err(failure.into()),
        };
        tracing::warn!(event = EVENT_UNUSABLE, workspace_id = %workspace, fault);
        Ok(Held::Unusable)
    }

    /// Writes a new key for `workspace` and retires every checkpoint the old
    /// one sealed: their generations are deleted, their objects' bucket deletes
    /// are owed, and the workspace's next save is a full one. `None` for a
    /// workspace that does not exist.
    ///
    /// # Errors
    /// A datastore or vault failure, with nothing changed.
    pub async fn rotate(&self, workspace: &Uuid7) -> Result<Option<RetiredCheckpoints>> {
        crate::durable_task::checkpoint::rotate(self, &self.database, workspace).await
    }

    /// Retires what a missing or unreadable key sealed.
    pub(crate) async fn retire(&self, workspace: &Uuid7) -> Result<RetiredCheckpoints> {
        crate::durable_task::checkpoint::retire(&self.database, workspace).await
    }

    /// The key a restore's frames carry, read on a connection of its own.
    pub(crate) async fn held(&self, workspace: &Uuid7) -> Result<Held> {
        let mut connection = self.database.acquire().await?;
        self.for_restore(&mut connection, workspace).await
    }

    /// Replaces the workspace's key with fresh bytes, or writes its first.
    /// Whoever calls this retires what the old key sealed.
    pub(crate) async fn replace(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        workspace: &Uuid7,
    ) -> Result<()> {
        let (name, body) = self.fresh()?;
        match self.vault.replace_in(tx, workspace, &name, &body).await {
            Ok(_nobody) => Ok(()),
            Err(absent) if absent.code() == error_code::SECRET_NOT_FOUND => {
                let _nobody = self
                    .vault
                    .create_in(tx, workspace, &name, &body, clock::now())
                    .await?;
                Ok(())
            }
            Err(failure) => Err(failure.into()),
        }
    }

    /// Logs a generation that the workspace's present key did not seal.
    pub(crate) fn mismatched(workspace: &Uuid7) {
        tracing::warn!(event = EVENT_UNUSABLE, workspace_id = %workspace, fault = "replaced");
    }

    async fn read(
        &self,
        connection: &mut PgConnection,
        workspace: &Uuid7,
    ) -> afd_vault::Result<std::result::Result<WorkspaceKey, BagFault>> {
        self.vault
            .bag_on::<WorkspaceKey>(connection, workspace, CHECKPOINT_KEY_NAME)
            .await
    }

    fn fresh(&self) -> Result<(SecretName, SecretBody)> {
        let mut material = Zeroizing::new([0_u8; af_crypto::KEY_LEN]);
        self.entropy.fill(material.as_mut_slice())?;
        let digits = Zeroizing::new(hex::encode(material.as_slice()));
        let document = Zeroizing::new(format!("{{\"{FIELD}\":\"{}\"}}", digits.as_str()));
        let object =
            SecretObject::parse(document.as_bytes()).map_err(af_core::json::Unreadable::from)?;
        Ok((
            SecretName::parse(CHECKPOINT_KEY_NAME)?,
            SecretBody::from_object(&object)?,
        ))
    }
}

/// A key row a save cannot use: it is never overwritten, and the save is refused.
fn unusable(workspace: &Uuid7, fault: &'static str) -> crate::Error {
    tracing::warn!(event = EVENT_UNUSABLE, workspace_id = %workspace, fault);
    DurableTaskRefusal::StorageUnavailable.into()
}
