//! Read-free bundle preview.
//!
//! A preview owns no vault or datastore handle by construction. Credential
//! names are declarations in `TRIGGER.md`; resolving their values here would
//! both violate the trust boundary and make preview availability depend on a
//! workspace the caller has not selected yet.

use crate::{ImportBody, Requirements, Result, SupportManifest, prepare};

/// Safe metadata returned before an import is persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    /// Fleet identity from `SKILL.md`.
    pub name: String,
    /// Human-readable skill description.
    pub description: String,
    /// Content identity the eventual snapshot will use.
    pub content_hash: String,
    /// Declared names and paths, without any credential values.
    pub requirements: Requirements,
    /// Content-free support-file metadata.
    pub support_manifest: Vec<SupportManifest>,
}

/// Pure preview service with no datastore or vault dependency.
#[derive(Debug, Clone, Copy, Default)]
pub struct Previewer;

impl Previewer {
    /// Validates the exact import input and returns safe metadata only.
    ///
    /// # Errors
    /// Returns the same validation error a persisted import would return.
    pub fn preview(self, body: &ImportBody) -> Result<Preview> {
        let prepared = prepare(body)?;
        Ok(Preview {
            name: prepared.name,
            description: prepared.description,
            content_hash: prepared.content_hash,
            requirements: prepared.requirements,
            support_manifest: prepared.support_manifest,
        })
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::expect_used, reason = "tests inspect successful fixtures")]

    use core::cell::Cell;

    use super::Previewer;
    use crate::{ImportBody, SourceKind};

    #[derive(Default)]
    struct VaultProbe(Cell<usize>);

    impl VaultProbe {
        fn reads(&self) -> usize {
            self.0.get()
        }
    }

    #[test]
    fn test_bundle_preview_no_vault() {
        let vault = VaultProbe::default();
        let input = ImportBody {
            source_kind: SourceKind::Upload,
            source_ref: "unit".into(),
            source_revision: None,
            skill_markdown: b"---\nname: mailer\ndescription: Sends mail\nversion: 1.0.0\n---\n".to_vec(),
            trigger_markdown: Some(b"---\nname: mailer\nx-agentsfleet:\n  triggers:\n    - type: cron\n      schedule: '0 * * * *'\n  credentials: [postmark]\n  tools: [http_request]\n  budget:\n    daily_dollars: 1\n---\n".to_vec()),
            support_files: Vec::new(),
        };

        let preview = Previewer.preview(&input).expect("the fixture is valid");

        assert_eq!(preview.requirements.credentials, ["postmark"]);
        assert_eq!(
            vault.reads(),
            0,
            "preview must not resolve credential names"
        );
    }
}
