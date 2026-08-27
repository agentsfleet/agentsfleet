//! Tenant-visible projection of the published platform catalogue.

use serde_json::Value;
use sqlx::Row as _;

use super::{Libraries, VISIBILITY_PUBLIC};
use crate::{Error, Result};

const CONTEXT_LIST: &str = "list published Fleet Bundles";

/// One published Fleet Bundle without source or snapshot internals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicLibraryItem {
    id: String,
    name: String,
    description: String,
    required_credentials: Vec<String>,
    required_credentials_reasons: Value,
    required_tools: Vec<String>,
    network_hosts: Vec<String>,
}

impl PublicLibraryItem {
    /// Stable catalogue slug.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Curated summary.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Credential names, never values.
    #[must_use]
    pub fn required_credentials(&self) -> &[String] {
        &self.required_credentials
    }

    /// Operator-authored reason copy keyed by credential name.
    #[must_use]
    pub const fn required_credentials_reasons(&self) -> &Value {
        &self.required_credentials_reasons
    }

    /// Tool identifiers declared by the bundle.
    #[must_use]
    pub fn required_tools(&self) -> &[String] {
        &self.required_tools
    }

    /// Outbound hosts declared by the bundle.
    #[must_use]
    pub fn network_hosts(&self) -> &[String] {
        &self.network_hosts
    }
}

impl Libraries {
    /// Lists every published row carrying a current bundle.
    ///
    /// # Errors
    /// Reports a database or malformed-JSON failure.
    pub async fn published(&self) -> Result<Vec<PublicLibraryItem>> {
        let mut connection = self.0.acquire().await?;
        sqlx::query(LIST)
            .bind(VISIBILITY_PUBLIC)
            .fetch_all(&mut *connection)
            .await
            .map_err(Error::database(CONTEXT_LIST))?
            .iter()
            .map(decode)
            .collect()
    }
}

fn decode(row: &sqlx::postgres::PgRow) -> Result<PublicLibraryItem> {
    let text =
        |index| -> Result<String> { row.try_get(index).map_err(Error::database(CONTEXT_LIST)) };
    Ok(PublicLibraryItem {
        id: text(0)?,
        name: text(1)?,
        description: text(2)?,
        required_credentials: serde_json::from_str(&text(3)?)?,
        required_credentials_reasons: serde_json::from_str(&text(4)?)?,
        required_tools: serde_json::from_str(&text(5)?)?,
        network_hosts: serde_json::from_str(&text(6)?)?,
    })
}

const LIST: &str = "SELECT id,name,description,required_credentials::text,required_credentials_reasons::text,required_tools::text,network_hosts::text FROM core.fleet_library WHERE visibility=$1 AND content_hash IS NOT NULL ORDER BY id";

#[cfg(test)]
mod tests {
    use super::LIST;

    #[test]
    fn published_projection_excludes_bundle_and_source_internals() {
        assert!(LIST.contains("visibility=$1 AND content_hash IS NOT NULL"));
        assert!(!LIST.contains("skill_markdown,"));
        assert!(!LIST.contains("trigger_markdown"));
        assert!(!LIST.contains("support_files_json"));
        assert!(!LIST.contains("source_repo"));
    }
}
