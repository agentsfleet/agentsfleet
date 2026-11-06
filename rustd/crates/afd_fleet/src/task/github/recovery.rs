//! Expiring exact-repository read custody survives loss of the live task owner.

use super::{BoundPr, Github, http::Session};
use crate::{
    Result,
    error::query,
    task::{TaskLease, TaskRefusal},
};
use afd_core::{clock, id::Uuid7};
use afd_credential::credential::outcome::Minted;
use afd_crypto::{
    aad::Aad,
    envelope::{Envelope, Sealer},
};
use afd_vault::StoredEnvelope;
use jiff_sqlx::ToSqlx as _;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use zeroize::Zeroizing;

#[derive(Serialize, Deserialize)]
struct Capability<'a> {
    token: &'a str,
    repository: &'a str,
    branch: &'a str,
    installation_id: u64,
    repository_id: u64,
}

pub(in crate::task) struct Sealed {
    envelope: Envelope,
    expires_at: jiff::Timestamp,
}

#[derive(sqlx::FromRow)]
pub(in crate::task) struct Stored {
    #[sqlx(flatten)]
    envelope: StoredEnvelope,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub expires_at: jiff::Timestamp,
}

pub(in crate::task) struct Opened<'a> {
    pub session: Session<'a>,
    pub repository: String,
    pub branch: String,
    pub repository_id: u64,
    pub installation_id: u64,
}

impl Github {
    pub(in crate::task) async fn seal_recovery(
        &self,
        lease: TaskLease<'_>,
        workspace: &Uuid7,
        effect: &Uuid7,
        target: &BoundPr,
    ) -> Result<Sealed> {
        let session = self.read_session(lease, &target.repository).await?;
        let credential = session.read_credential()?;
        if session.installation_id != target.installation_id
            || credential.expires_at <= clock::now()
        {
            return Err(TaskRefusal::InvalidTarget.into());
        }
        let material = Zeroizing::new(
            serde_json::to_vec(&Capability {
                token: credential.token.as_str(),
                repository: &target.repository,
                branch: &target.branch,
                installation_id: target.installation_id,
                repository_id: target.repository_id,
            })
            .map_err(afd_core::json::Unreadable::from)?,
        );
        let envelope = Sealer::new().seal(
            &self.master,
            &Aad::effect_recovery(workspace, effect),
            &material,
        )?;
        Ok(Sealed {
            envelope,
            expires_at: credential.expires_at,
        })
    }

    pub(in crate::task) fn open_recovery(
        &self,
        workspace: &Uuid7,
        effect: &Uuid7,
        stored: Stored,
    ) -> Result<Opened<'_>> {
        if stored.expires_at <= clock::now() {
            return Err(TaskRefusal::EffectUncertain.into());
        }
        let plaintext = stored
            .envelope
            .into_envelope()?
            .open(&self.master, &Aad::effect_recovery(workspace, effect))?;
        let capability: Capability<'_> =
            serde_json::from_slice(plaintext.expose()).map_err(afd_core::json::Unreadable::from)?;
        let credential = Minted {
            token: Zeroizing::new(capability.token.to_owned()),
            expires_at: stored.expires_at,
            rotated_refresh_token: None,
        };
        Ok(Opened {
            session: Session::new(&self.client, credential, capability.installation_id, true),
            repository: capability.repository.to_owned(),
            branch: capability.branch.to_owned(),
            repository_id: capability.repository_id,
            installation_id: capability.installation_id,
        })
    }
}

impl Sealed {
    pub(in crate::task) async fn insert(
        &self,
        connection: &mut PgConnection,
        workspace: &Uuid7,
        effect: &Uuid7,
    ) -> Result<()> {
        let envelope = &self.envelope;
        sqlx::query(include_str!("sql/recovery_insert.sql"))
            .bind(effect.as_str())
            .bind(workspace.as_str())
            .bind(envelope.wrapped_dek())
            .bind(envelope.dek_nonce().as_slice())
            .bind(envelope.dek_tag().as_slice())
            .bind(envelope.payload_nonce().as_slice())
            .bind(envelope.payload_ciphertext())
            .bind(envelope.payload_tag().as_slice())
            .bind(envelope.kek_version())
            .bind(self.expires_at.to_sqlx())
            .bind(3600_f64)
            .execute(connection)
            .await
            .map_err(query("retain expiring read-only reconciliation custody"))?;
        Ok(())
    }
}
