//! Setup inspection extracts only connector identity; unrelated secret fields are skipped.

use afd_core::id::Uuid7;
use afd_vault::StoredEnvelope;
use sqlx::PgConnection;

use super::{KeyRef, Vault};
use crate::{
    error::{Result, query},
    secrets::{Connector, Connectors, Supply},
};

mod handle;

use handle::Handle;

const CONTEXT: &str = "inspect declared credential access";

/// The declared set's envelopes under a share lock, each with a generation
/// digest bound to its ciphertext.
///
/// `$1` workspace · `$2` the declared names.
const INSPECT: &str = include_str!("inspect.sql");

/// One declared credential, as [`INSPECT`] names its columns.
#[derive(Debug, sqlx::FromRow)]
struct InspectedRow {
    key_name: String,
    #[sqlx(flatten)]
    envelope: StoredEnvelope,
    resolution_version: String,
}

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
    /// The signing field parses as the Svix key used by ingress.
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
        let rows = sqlx::query_as::<_, InspectedRow>(INSPECT)
            .bind(workspace.as_str())
            .bind(names)
            .fetch_all(connection)
            .await
            .map_err(query(CONTEXT))?;
        rows.into_iter()
            .map(|row| self.inspect_row(row, workspace, connectors))
            .collect()
    }

    fn inspect_row(
        &self,
        row: InspectedRow,
        workspace: &Uuid7,
        connectors: &dyn Connectors,
    ) -> Result<CredentialInspection> {
        let InspectedRow {
            key_name: name,
            envelope,
            resolution_version,
        } = row;
        let plaintext = self.decrypt(
            envelope,
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
            svix_usable: handle.as_ref().is_some_and(Handle::svix_usable),
            connection_id,
            integration: connector
                .map(|connector| connector.name().to_owned())
                .or_else(|| {
                    integration.filter(|name| name == crate::secrets::connector::SLACK_INTEGRATION)
                }),
            resolution_version,
        })
    }
}
