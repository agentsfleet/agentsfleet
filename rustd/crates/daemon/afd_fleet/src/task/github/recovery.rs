//! Expiring exact-repository read custody survives loss of the live task owner.

use super::{BoundPr, Github};
use crate::{
    Result,
    error::query,
    task::{TaskRefusal, TaskRun},
};
use af_core::{clock, id::Uuid7};
use af_crypto::{
    aad::Aad,
    envelope::{Envelope, Sealer},
};
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

impl Github {
    pub(in crate::task) async fn seal_recovery(
        &self,
        run: TaskRun<'_>,
        workspace: &Uuid7,
        effect: &Uuid7,
        target: &BoundPr,
    ) -> Result<Sealed> {
        let session = self.read_session(run, &target.repository).await?;
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
            .map_err(af_core::json::Unreadable::from)?,
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
            .bind(effect)
            .bind(workspace)
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
