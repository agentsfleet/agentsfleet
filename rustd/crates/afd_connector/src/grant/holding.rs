//! What a workspace HOLDS once a connect has landed: whether the handle is
//! there, what a person sees it called, and letting go of it.
//!
//! The read half of [`super::Grants`], where the module above is the write
//! half. A child module rather than a sibling because it acts through the same
//! two private handles the landing does — the vault the grant is sealed in and
//! the pool the routing rows live on — and Rust already scopes those to this
//! subtree. Reaching them from a sibling would need accessors, and an accessor
//! on a store is an invitation to make some other call through it.
//!
//! # Answering a question about a connection rarely opens an envelope
//!
//! The catalogue answer comes from [`afd_vault::Directory`], which holds no
//! key: it asks which names exist and nothing else, so rendering the catalogue
//! decrypts no credential the workspace holds.
//!
//! Two reads DO open one row each, and they are opening it for different
//! things. [`Grants::connection`] opens a handle to read the label a person
//! sees — public data that happens to be stored beside a secret. [`Grants::
//! bot_identity`] opens it for the secret itself, has exactly one caller
//! (`afd_outbound`, posting a fleet's answer back), and hands the bytes on
//! still wrapped so they zero on drop. The split is the point: [`Connection`]
//! carries no field that could hold a token, so a status surface reaching for
//! one does not compile.
//!
//! # A disconnect leaves the provider alone
//!
//! It removes this daemon's sealed handle and the rows that route the
//! provider's events back, and it revokes nothing at the vendor. That is the
//! property that makes reconnecting always available: a person whose token was
//! revoked upstream, or whose install drifted, presses Connect again and the
//! flow starts clean.

use std::collections::BTreeSet;

use afd_core::id::Uuid7;
use afd_core::json::non_empty_str;
use afd_crypto::secret::SecretString;
use afd_vault::{Deleted, SecretName};
use sqlx::Acquire as _;

use super::Grants;
use super::parse::{HANDLE_BOT_TOKEN, HANDLE_BOT_USER_ID, HANDLE_INTEGRATION, HANDLE_LABEL};
use crate::error::{Result, query};
use crate::provider::Provider;
use crate::sql;

/// The bot a workspace's grant speaks as.
///
/// Not `Clone`: the token is zeroed on drop, and a copy would outlive that.
#[derive(Debug)]
pub struct BotIdentity {
    /// What a call to the provider's API spends.
    pub token: SecretString,
    /// The id the provider knows the bot by, when the grant recorded one.
    pub user_id: Option<String>,
}

/// The context a failed routing-row delete reports under.
const CONTEXT_FORGET: &str = "forget a workspace's connector routing rows";

/// One workspace's connection to one provider, as a reader sees it.
///
/// Carries no token and no field that could hold one: the whole point of this
/// type is that the status surface answers from it, so a field added here is a
/// field a dashboard renders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    /// What a person sees this connection called — the workspace, site or team
    /// the grant is scoped to.
    ///
    /// `None` for a landed handle that carries no label, which is a real state
    /// rather than a broken one: a provider whose answer named nothing still
    /// connected.
    pub label: Option<String>,
}

/// What a disconnect found.
///
/// A value rather than a `bool`, for the reason [`afd_vault::Deleted`] is one:
/// both outcomes answer 204, so nothing at the edge would force a caller to
/// remember which way round the boolean read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forgotten {
    /// A handle was held and is now gone.
    Disconnected,
    /// This workspace held no handle for the provider. The request got what it
    /// wanted.
    AlreadyAbsent,
}

impl Grants {
    /// This workspace's connection to `provider`, or nothing.
    ///
    /// `None` for every shape that is not a landed grant — no row, a body that
    /// is not an object, an object with no [`HANDLE_INTEGRATION`] marker. The
    /// marker is what separates a handle this daemon wrote from any other
    /// document stored under the same name, and a reader that skipped it would
    /// report a workspace as connected because somebody stored a secret called
    /// `slack`.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and an envelope that would not
    /// open.
    pub async fn connection(
        &self,
        workspace: &Uuid7,
        provider: Provider,
    ) -> Result<Option<Connection>> {
        let handle = self.landed_handle(workspace, provider).await?;
        Ok(handle.map(|handle| Connection {
            label: non_empty_str(&handle, HANDLE_LABEL).map(str::to_owned),
        }))
    }

    /// Which providers this workspace holds a handle for.
    ///
    /// One listing, no decryption: a connector's grant key IS its provider id
    /// (see [`Provider::grant_key`]), so membership answers the whole catalogue
    /// column without opening a single envelope.
    ///
    /// A name that is not a shipped provider — an ordinary workspace secret —
    /// simply matches nothing, which is why this filters the registry by the
    /// listing rather than the listing by the registry.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn held(&self, workspace: &Uuid7) -> Result<BTreeSet<Provider>> {
        let stored: BTreeSet<String> = self
            .vault
            .directory()
            .list(workspace)
            .await?
            .into_iter()
            .map(|secret| secret.name)
            .collect();

        Ok(Provider::ALL
            .iter()
            .copied()
            .filter(|provider| stored.contains(provider.grant_key()))
            .collect())
    }

    /// The bot this workspace's grant speaks as: its token, and the user id the
    /// provider knows it by.
    ///
    /// The ONE read in this module that opens an envelope for what is inside
    /// it rather than to answer a question about it, and it exists for exactly
    /// one caller: `afd_outbound`, delivering a fleet's answer back to the
    /// place the question came from. Nothing on the request path calls it —
    /// [`Connection`] is deliberately shaped so a status surface cannot.
    ///
    /// The id is what lets an inbound mention recognise the bot's own messages
    /// and its own leading mention; the token is what a thread re-read spends.
    /// One read, so the two can never come from different grants.
    ///
    /// The token is a [`SecretString`] because it is zeroed on drop, and a
    /// `String` would silently end that: the caller builds one `Authorization`
    /// header from it and has no reason to keep a copy.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and an envelope that would not
    /// open. `None` for every shape that is not a landed grant carrying a token
    /// — no handle, a body that is not an object, an object with no
    /// [`HANDLE_INTEGRATION`] marker, a token field that is absent or empty;
    /// all mean this workspace cannot post as itself until somebody
    /// reconnects. A grant with a token and no user id answers the token with
    /// no id rather than nothing.
    pub async fn bot_identity(
        &self,
        workspace: &Uuid7,
        provider: Provider,
    ) -> Result<Option<BotIdentity>> {
        let handle = self.landed_handle(workspace, provider).await?;
        Ok(handle.and_then(|handle| {
            non_empty_str(&handle, HANDLE_BOT_TOKEN).map(|token| BotIdentity {
                token: SecretString::new(token.to_owned()),
                user_id: non_empty_str(&handle, HANDLE_BOT_USER_ID).map(str::to_owned),
            })
        }))
    }

    /// The handle this daemon landed for `provider`, or `None` for any other
    /// shape: no row, a body that is not JSON, or JSON without the
    /// [`HANDLE_INTEGRATION`] marker.
    ///
    /// The marker is what separates a handle this daemon wrote from any other
    /// document stored under the same name.
    async fn landed_handle(
        &self,
        workspace: &Uuid7,
        provider: Provider,
    ) -> Result<Option<serde_json::Value>> {
        let name = SecretName::parse(provider.grant_key())?;
        let Some(stored) = self.vault.load(workspace, &name).await? else {
            return Ok(None);
        };
        let handle = serde_json::from_slice::<serde_json::Value>(stored.expose()).ok();
        Ok(handle.filter(|handle| handle.get(HANDLE_INTEGRATION).is_some()))
    }

    /// Forgets this workspace's connection to `provider`.
    ///
    /// One transaction over the sealed handle and the routing rows, because
    /// both live in this Postgres and either alone is a state a reader cannot
    /// detect: rows naming an account with no credential behind them, or a
    /// credential a second workspace can claim the account away from while it
    /// still mints. Unordered against a concurrent [`Grants::land`]; see the
    /// module note.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a vault that refused the
    /// delete; either rolls back both, so the connection is left as it was. A
    /// workspace holding no handle is [`Forgotten::AlreadyAbsent`], not an
    /// error — the caller wanted it gone and it is gone.
    pub async fn forget(&self, workspace: &Uuid7, provider: Provider) -> Result<Forgotten> {
        let name = SecretName::parse(provider.grant_key())?;
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_FORGET))?;
        let forgotten = match self
            .vault
            .directory()
            .delete_in(&mut transaction, workspace, &name)
            .await?
        {
            Deleted::Removed => Forgotten::Disconnected,
            Deleted::AlreadyAbsent => Forgotten::AlreadyAbsent,
        };
        sqlx::query(sql::DELETE_WORKSPACE_INSTALLS)
            .bind(provider.id())
            .bind(workspace.as_str())
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_FORGET))?;
        transaction.commit().await.map_err(query(CONTEXT_FORGET))?;

        // `delete_in` leaves logging to its caller, so this line also says
        // whether a credential went: the vault's own `secret_deleted` is not
        // emitted on this path.
        let credential_removed = matches!(forgotten, Forgotten::Disconnected);
        let (workspace_id, provider) = (workspace.as_str(), provider.id());
        tracing::info!(
            workspace_id,
            provider,
            credential_removed,
            event = "connector_disconnected",
        );
        Ok(forgotten)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::{HANDLE_INTEGRATION, HANDLE_LABEL, non_empty_str};

    /// A handle with no label still names a connection.
    ///
    /// Four of the five connectors land a label; the shape that does not must
    /// read as connected-without-a-name rather than as no connection, because
    /// the marker and the label answer different questions.
    #[test]
    fn a_labelless_handle_is_still_a_connection() {
        let handle: serde_json::Value =
            serde_json::from_str(r#"{"integration":"slack"}"#).expect("valid JSON fixture");

        assert!(handle.get(HANDLE_INTEGRATION).is_some());
        assert_eq!(non_empty_str(&handle, HANDLE_LABEL), None);
    }

    /// No unusable label shape reaches a dashboard card.
    ///
    /// The empty string is the one worth pinning: it is present, so a reader
    /// checking only for presence would render a card with a blank name.
    #[test]
    fn no_unusable_label_shape_is_rendered() {
        for stored in [
            r#"{"label":""}"#,
            r#"{"label":null}"#,
            r#"{"label":42}"#,
            r#"{"label":{"nested":"x"}}"#,
            "{}",
        ] {
            let handle: serde_json::Value =
                serde_json::from_str(stored).expect("valid JSON fixture");
            assert_eq!(
                non_empty_str(&handle, HANDLE_LABEL),
                None,
                "`{stored}` names no label"
            );
        }
    }

    /// A label that IS set is read verbatim.
    #[test]
    fn a_set_label_is_read_as_written() {
        let handle: serde_json::Value =
            serde_json::from_str(r#"{"integration":"jira","label":"Acme Site"}"#)
                .expect("valid JSON fixture");

        assert_eq!(non_empty_str(&handle, HANDLE_LABEL), Some("Acme Site"));
    }
}
