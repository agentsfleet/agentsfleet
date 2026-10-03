//! Pure preparation of validated bundle metadata.

use afd_core::bundle::BundleDigest;
use garde::{Unvalidated, Valid};
use sha2::{Digest as _, Sha256};

use crate::error::{InvalidBundle, Result};
use crate::frontmatter;
use crate::model::{ImportBody, PreparedBundle, Requirements, SupportManifest};

const SNAPSHOT_PREFIX: &str = "fleet-bundles/sha256/";

/// Validates untrusted bytes and derives metadata without performing I/O.
///
/// # Errors
/// Returns [`crate::Error::Invalid`] for the first violated bundle rule.
pub fn prepare(body: &ImportBody) -> Result<PreparedBundle> {
    crate::validate::body(body)?;
    let skill = frontmatter::skill(&body.skill_markdown)?;
    let requirements = requirements(body, &skill.name)?;
    let (content_hash, support_manifest) = hashes(body);
    Ok(PreparedBundle {
        name: skill.name,
        description: skill.description,
        snapshot_key: format!("{SNAPSHOT_PREFIX}{content_hash}.tar"),
        content_hash,
        support_manifest,
        requirements,
    })
}

fn requirements(body: &ImportBody, skill_name: &str) -> Result<Requirements> {
    let support_files = body
        .support_files
        .iter()
        .map(|file| file.path.clone())
        .collect();
    let Some(markdown) = body.trigger_markdown.as_deref() else {
        return Ok(Requirements {
            credentials: Vec::new(),
            tools: Vec::new(),
            network_hosts: Vec::new(),
            support_files,
            trigger_present: false,
        });
    };
    let trigger = frontmatter::trigger(markdown)?;
    if trigger.name().as_str() != skill_name {
        return Err(InvalidBundle::NameMismatch.into());
    }
    let credentials = trigger
        .credentials()
        .iter()
        .map(|value| value.as_str().to_owned())
        .collect::<Vec<_>>();
    let tools = trigger
        .tools()
        .iter()
        .map(|value| value.as_ref().to_owned())
        .collect::<Vec<_>>();
    let network_hosts: Vec<String> = trigger
        .network()
        .map(|network| {
            network
                .allow()
                .iter()
                .map(|value| value.as_ref().to_owned())
                .collect()
        })
        .unwrap_or_default();
    Unvalidated::new(Requirements {
        credentials,
        tools,
        network_hosts,
        support_files,
        trigger_present: true,
    })
    .validate()
    .map(Valid::into_inner)
    .map_err(|_report| InvalidBundle::RequirementsTooLarge.into())
}

fn hashes(body: &ImportBody) -> (String, Vec<SupportManifest>) {
    let mut bundle = BundleDigest::new(
        body.skill_markdown.as_ref(),
        body.trigger_markdown.as_ref().map(AsRef::as_ref),
    );
    let manifest = body
        .support_files
        .iter()
        .map(|file| {
            bundle.support_file(&file.path, &file.content);
            SupportManifest {
                path: file.path.clone(),
                size_bytes: file.content.len(),
                sha256: hex::encode(Sha256::digest(&file.content)),
            }
        })
        .collect();
    (bundle.finish(), manifest)
}

#[cfg(test)]
mod tests;
