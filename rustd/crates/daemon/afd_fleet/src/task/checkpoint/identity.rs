//! Workspace/repository identity and quota are shared by a bounded object batch.

use crate::{Result, error::query, run::Runs, task::storage::R2CheckpointStore};
use af_core::{clock, id::Uuid7};
use af_crypto::{aad::Aad, envelope::Sealer, mac::HmacSha256Tag, secret::SecretBytes};
use af_wire::task::CheckpointPlaintext;
use afd_vault::StoredEnvelope;
use sqlx::PgConnection;

pub(crate) struct Identity {
    pub id: Uuid7,
    pub key: SecretBytes,
}

#[derive(sqlx::FromRow)]
struct Stored {
    id: Uuid7,
    key_version: i32,
    #[sqlx(flatten)]
    envelope: StoredEnvelope,
}

impl Identity {
    pub(crate) fn content(&self, descriptor: &CheckpointPlaintext, format: i32) -> String {
        HmacSha256Tag::compute_peppered(
            &self.key,
            &[
                b"\0agentsfleet.content.v1\0",
                &format.to_be_bytes(),
                descriptor.sha256.as_bytes(),
                &descriptor.size.to_be_bytes(),
            ],
        )
        .to_hex()
    }
}

pub(crate) async fn quota(connection: &mut PgConnection, workspace: &Uuid7) -> Result<()> {
    sqlx::query_scalar::<_, Uuid7>(include_str!("sql/quota.sql"))
        .bind(workspace)
        .fetch_one(connection)
        .await
        .map_err(query("lock workspace checkpoint quota"))?;
    Ok(())
}

pub(crate) async fn load(
    runs: &Runs,
    storage: &R2CheckpointStore,
    connection: &mut PgConnection,
    workspace: &Uuid7,
    repository: &str,
) -> Result<Identity> {
    let stored: Option<Stored> = sqlx::query_as(include_str!("sql/identity.sql"))
        .bind(workspace)
        .bind(repository)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("lock active repository identity key"))?;
    if let Some(stored) = stored {
        let id = stored.id.clone();
        let envelope = stored.envelope.into_envelope()?;
        let key = envelope.open(
            &storage.master,
            &Aad::checkpoint_identity(
                workspace,
                &id,
                repository,
                stored.key_version,
                envelope.kek_version(),
            ),
        )?;
        return Ok(Identity { id, key });
    }
    create(runs, storage, connection, workspace, repository).await
}

async fn create(
    runs: &Runs,
    storage: &R2CheckpointStore,
    connection: &mut PgConnection,
    workspace: &Uuid7,
    repository: &str,
) -> Result<Identity> {
    let id = runs.entropy().uuid7(clock::now())?;
    let key_version: i32 = sqlx::query_scalar(include_str!("sql/identity_version.sql"))
        .bind(workspace)
        .bind(repository)
        .fetch_one(&mut *connection)
        .await
        .map_err(query("allocate repository identity version"))?;
    let mut material = zeroize::Zeroizing::new([0_u8; af_crypto::KEY_LEN]);
    runs.entropy().fill(material.as_mut_slice())?;
    let envelope = Sealer::new().seal(
        &storage.master,
        &Aad::checkpoint_identity(
            workspace,
            &id,
            repository,
            key_version,
            af_crypto::KEK_VERSION,
        ),
        material.as_slice(),
    )?;
    sqlx::query(include_str!("sql/identity_insert.sql"))
        .bind(&id)
        .bind(workspace)
        .bind(repository)
        .bind(key_version)
        .bind(envelope.wrapped_dek())
        .bind(envelope.dek_nonce().as_slice())
        .bind(envelope.dek_tag().as_slice())
        .bind(envelope.payload_nonce().as_slice())
        .bind(envelope.payload_ciphertext())
        .bind(envelope.payload_tag().as_slice())
        .bind(envelope.kek_version())
        .execute(connection)
        .await
        .map_err(query("retain wrapped repository identity key"))?;
    Ok(Identity {
        id,
        key: SecretBytes::new(material.to_vec()),
    })
}
