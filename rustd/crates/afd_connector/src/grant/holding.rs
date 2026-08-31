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
//! key: it asks which names exist and nothing else. `catalog.zig` decrypts
//! every credential a workspace holds to answer the same question, which is
//! what made its own budget comment necessary.
//!
//! Two reads DO open one row each, and they are opening it for different
//! things. [`Grants::connection`] opens a handle to read the label a person
//! sees — public data that happens to be stored beside a secret. [`Grants::
//! bot_token`] opens it for the secret itself, has exactly one caller
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
//! flow starts clean. `disconnect.zig` states the same rule.

use std::collections::BTreeSet;

use afd_core::id::Uuid7;
use afd_crypto::secret::SecretBytes;
use afd_vault::{Deleted, SecretName};

use super::Grants;
use super::parse::{HANDLE_BOT_TOKEN, HANDLE_INTEGRATION, HANDLE_LABEL};
use crate::error::{Result, query};
use crate::provider::Provider;
use crate::sql;

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
    /// open. A name the vault would refuse is `None` rather than an error: it
    /// is derived from a shipped provider id, so a refusal would be this
    /// build's bug and not something to tell a person reading a status page.
    pub async fn connection(
        &self,
        workspace: &Uuid7,
        provider: Provider,
    ) -> Result<Option<Connection>> {
        let Ok(name) = SecretName::parse(provider.grant_key()) else {
            return Ok(None);
        };
        let Some(stored) = self.vault.load(workspace, &name).await? else {
            return Ok(None);
        };
        let Ok(handle) = serde_json::from_slice::<serde_json::Value>(stored.expose()) else {
            return Ok(None);
        };
        if handle.get(HANDLE_INTEGRATION).is_none() {
            return Ok(None);
        }
        Ok(Some(Connection {
            label: text(&handle, HANDLE_LABEL),
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

    /// The bot token this workspace's grant holds for `provider`.
    ///
    /// The ONE read in this module that opens an envelope for what is inside
    /// it rather than to answer a question about it, and it exists for exactly
    /// one caller: `afd_outbound`, delivering a fleet's answer back to the
    /// place the question came from. Nothing on the request path calls it —
    /// [`Connection`] is deliberately shaped so a status surface cannot.
    ///
    /// # Why the token stays [`SecretBytes`]
    ///
    /// It is zeroed on drop, and handing back a `String` would silently end
    /// that: the caller builds one `Authorization` header from it and has no
    /// reason to keep a copy. The vault's own note says a caller that copies
    /// the bytes owns what happens next, and this one does not copy them.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and an envelope that would not
    /// open. `None` for every shape that is not a landed grant carrying a token
    /// — no handle, a body that is not an object, an object with no
    /// [`HANDLE_INTEGRATION`] marker, a token field that is absent or empty.
    /// A caller cannot act differently on any of them: all four mean this
    /// workspace cannot post as itself until somebody reconnects.
    pub async fn bot_token(
        &self,
        workspace: &Uuid7,
        provider: Provider,
    ) -> Result<Option<SecretBytes>> {
        let Ok(name) = SecretName::parse(provider.grant_key()) else {
            return Ok(None);
        };
        let Some(stored) = self.vault.load(workspace, &name).await? else {
            return Ok(None);
        };
        let Ok(handle) = serde_json::from_slice::<serde_json::Value>(stored.expose()) else {
            return Ok(None);
        };
        if handle.get(HANDLE_INTEGRATION).is_none() {
            return Ok(None);
        }
        Ok(text(&handle, HANDLE_BOT_TOKEN).map(|token| SecretBytes::new(token.into_bytes())))
    }

    /// Forgets this workspace's connection to `provider`.
    ///
    /// The routing rows go FIRST and the sealed handle second, which is the
    /// reverse of the order [`Grants::land`] writes them in and deliberately
    /// so: both orders leave one intermediate state, and the ones they leave
    /// are not equally bad. Removing the handle first would leave rows saying
    /// a provider account belongs to this workspace with no credential behind
    /// them — an ingress that resolves a workspace and then cannot answer.
    /// This way the intermediate state is a handle nothing routes to, which is
    /// what a workspace that never installed the app is already in.
    ///
    /// Not a transaction, and that is the honest shape rather than a
    /// compromise: the two writes are in different stores, so a transaction
    /// over the pool would cover the rows and not the vault row it is ordered
    /// against. `binding_tx.zig` takes an advisory lock to make the pair look
    /// atomic and still cannot include the vault write.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a vault that refused the
    /// delete. A workspace holding no handle is [`Forgotten::AlreadyAbsent`],
    /// not an error — the caller wanted it gone and it is gone.
    pub async fn forget(&self, workspace: &Uuid7, provider: Provider) -> Result<Forgotten> {
        let mut connection = self.database.acquire().await?;
        sqlx::query(sql::DELETE_WORKSPACE_INSTALLS)
            .bind(provider.id())
            .bind(workspace.as_str())
            .execute(connection.as_mut())
            .await
            .map_err(query(CONTEXT_FORGET))?;
        drop(connection);

        let Ok(name) = SecretName::parse(provider.grant_key()) else {
            return Ok(Forgotten::AlreadyAbsent);
        };
        let forgotten = match self.vault.directory().delete(workspace, &name).await? {
            Deleted::Removed => Forgotten::Disconnected,
            Deleted::AlreadyAbsent => Forgotten::AlreadyAbsent,
        };

        tracing::info!(
            workspace_id = workspace.as_str(),
            provider = provider.id(),
            event = "connector_disconnected",
        );
        Ok(forgotten)
    }
}

/// One non-empty string field of a handle.
///
/// The same reading [`crate::app`] does over its own bag: a field that is
/// present and empty is a field nobody set, and rendering `""` as a label would
/// put a blank name on a dashboard card.
fn text(handle: &serde_json::Value, name: &str) -> Option<String> {
    let value = handle.get(name)?.as_str()?;
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::{HANDLE_INTEGRATION, HANDLE_LABEL, text};

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
        assert_eq!(text(&handle, HANDLE_LABEL), None);
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
                text(&handle, HANDLE_LABEL),
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

        assert_eq!(text(&handle, HANDLE_LABEL).as_deref(), Some("Acme Site"));
    }
}
