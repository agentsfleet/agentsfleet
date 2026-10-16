//! Setup inspection extracts only connector identity; unrelated secret fields are skipped.

use afd_core::id::Uuid7;
use sqlx::{PgConnection, Row as _, postgres::PgRow};

use super::{KeyRef, Vault};
use crate::{
    error::{Result, query},
    secrets::{Connector, Connectors, Supply},
};

mod handle;

use handle::Handle;

const CONTEXT: &str = "inspect declared credential access";
const INSPECT: &str = "\
SELECT key_name, encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, kek_version,
encode(sha256(convert_to(jsonb_build_array(id, kek_version, encode(sha256(ciphertext),'hex'))::text,'UTF8')),'hex') AS resolution_version
FROM vault.secrets WHERE workspace_id=$1::uuid AND key_name=ANY($2::text[])
ORDER BY key_name FOR SHARE";

/// Non-secret facts extracted from one declared credential under its row lock.
#[derive(Debug, Clone)]
pub struct CredentialInspection {
    /// Declared workspace name.
    pub name: String,
    /// Recognized connector, absent for an ordinary inline secret.
    pub integration: Option<String>,
    /// Existing runtime classification, using the daemon's connector registry.
    pub supply: Supply,
    /// Ciphertext-bound generation, without returning the ciphertext.
    pub resolution_version: String,
    /// Stored object and connector-name binding satisfy runtime admission rules.
    pub usable: bool,
    /// The stored object contains the nonempty signing field used by keyed ingress.
    pub webhook_usable: bool,
    /// Raw stored bytes parse as the signing key used by Svix ingress.
    pub svix_usable: bool,
    /// Public provider installation identity, never an access token.
    pub connection_id: Option<String>,
}

impl Vault {
    /// Inspects a bounded declared set on the caller's authorization transaction.
    ///
    /// Missing names stay absent. The envelope is opened only to classify its
    /// connector; token presence is checked in a zeroizing string, never a value tree.
    ///
    /// # Errors
    /// Preserves query and envelope failures; malformed stored objects are refused.
    pub async fn inspect_on(
        &self,
        connection: &mut PgConnection,
        workspace: &Uuid7,
        names: &[String],
        connectors: &dyn Connectors,
    ) -> Result<Vec<CredentialInspection>> {
        let rows = sqlx::query(INSPECT)
            .bind(workspace.as_str())
            .bind(names)
            .fetch_all(connection)
            .await
            .map_err(query(CONTEXT))?;
        rows.iter()
            .map(|row| self.inspect_row(row, workspace, connectors))
            .collect()
    }

    fn inspect_row(
        &self,
        row: &PgRow,
        workspace: &Uuid7,
        connectors: &dyn Connectors,
    ) -> Result<CredentialInspection> {
        let name: String = row.try_get("key_name").map_err(query(CONTEXT))?;
        let plaintext = self.decrypt(
            row,
            KeyRef {
                workspace_id: workspace,
                name: &name,
            },
        )?;
        let handle = afd_core::json::object_from_slice::<Handle<'_>>(plaintext.expose()).ok();
        let integration = handle.as_ref().and_then(Handle::integration);
        let connector = integration
            .as_deref()
            .and_then(|name| connectors.resolve(name));
        let supply = connector.map_or(Supply::Inline, Connector::supply);
        let connection_id = handle.as_ref().and_then(Handle::connection_id);
        let usable = handle
            .as_ref()
            .is_some_and(|handle| handle.usable(&name, supply, connector.map(Connector::exchange)));
        Ok(CredentialInspection {
            name,
            supply,
            usable,
            webhook_usable: handle.as_ref().is_some_and(Handle::webhook_usable),
            svix_usable: std::str::from_utf8(plaintext.expose())
                .ok()
                .and_then(afd_webhook::vendor::svix::SvixSecret::parse)
                .is_some(),
            connection_id,
            integration: connector
                .map(|connector| connector.name().to_owned())
                .or_else(|| {
                    integration.filter(|name| name == crate::secrets::connector::SLACK_INTEGRATION)
                }),
            resolution_version: row.try_get("resolution_version").map_err(query(CONTEXT))?,
        })
    }
}
