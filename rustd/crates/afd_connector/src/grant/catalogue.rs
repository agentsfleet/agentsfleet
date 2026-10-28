//! Workspace connector metadata, read with a bounded batch of registered names.

use super::{Connection, Grants};
use crate::{Provider, Result};
use afd_core::id::Uuid7;
use afd_vault::SecretName;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Metadata<'a> {
    #[serde(borrow)]
    integration: Option<std::borrow::Cow<'a, str>>,
    #[serde(borrow)]
    label: Option<&'a serde_json::value::RawValue>,
    #[serde(borrow)]
    team_name: Option<&'a serde_json::value::RawValue>,
}

impl Connection {
    /// Reads public metadata without materialising token fields.
    pub(super) fn from_bytes(provider: Provider, bytes: &[u8]) -> Option<Self> {
        let metadata: Metadata<'_> = serde_json::from_slice(bytes).ok()?;
        (metadata.integration.as_deref() == Some(provider.id())).then(|| Self {
            label: metadata
                .label
                .or_else(|| {
                    (provider == Provider::Slack)
                        .then_some(metadata.team_name)
                        .flatten()
                })
                .and_then(|label| serde_json::from_str::<String>(label.get()).ok())
                .filter(|label| !label.is_empty()),
        })
    }
}

impl Grants {
    /// Landed handles and their public labels, with one storage query.
    ///
    /// Only registered grant names are opened. Borrowed metadata avoids copying
    /// tokens into a JSON tree; the opened bytes are zeroed when dropped.
    ///
    /// # Errors
    /// Reports a failed workspace read or envelope open.
    pub async fn connections(&self, workspace: &Uuid7) -> Result<BTreeMap<Provider, Connection>> {
        let names = Provider::ALL
            .iter()
            .map(|provider| SecretName::parse(provider.grant_key()))
            .collect::<afd_vault::Result<Vec<_>>>()?;
        let opened = self.vault.load_many(workspace, &names).await?;
        Ok(Provider::ALL
            .iter()
            .filter_map(|provider| {
                let bytes = opened.get(provider.grant_key())?;
                Connection::from_bytes(*provider, bytes.expose())
                    .map(|connection| (*provider, connection))
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::Connection;
    use crate::Provider;

    #[test]
    fn only_the_matching_connector_marker_identifies_a_connection() {
        for bytes in [
            b"{}".as_slice(),
            br#"{"integration":"jira","label":"Other"}"#,
            br#"{"integration":true}"#,
            b"null",
            b"not-json",
        ] {
            assert_eq!(Connection::from_bytes(Provider::Slack, bytes), None);
        }
        assert_eq!(
            Connection::from_bytes(
                Provider::Slack,
                br#"{"integration":"slack","label":"Support \"Desk\"","bot_token":"hidden"}"#
            ),
            Some(Connection {
                label: Some("Support \"Desk\"".into())
            })
        );
    }

    #[test]
    fn an_unusable_label_does_not_hide_a_saved_connection() {
        for label in ["null", "42", "{}", "\"\""] {
            let bytes = format!(r#"{{"integration":"slack","label":{label}}}"#);
            assert_eq!(
                Connection::from_bytes(Provider::Slack, bytes.as_bytes()),
                Some(Connection { label: None })
            );
        }
    }

    #[test]
    fn slack_team_names_are_public_connection_labels() {
        let slack = br#"{"integration":"slack","team_name":"Support Desk","bot_token":"hidden"}"#;
        assert_eq!(
            Connection::from_bytes(Provider::Slack, slack),
            Some(Connection {
                label: Some("Support Desk".into())
            })
        );
        let jira = br#"{"integration":"jira","team_name":"Unrelated field"}"#;
        assert_eq!(
            Connection::from_bytes(Provider::Jira, jira),
            Some(Connection { label: None })
        );
    }
}
