//! Pure preparation of validated bundle metadata.

use sha2::{Digest as _, Sha256};

use crate::error::{InvalidBundle, Result};
use crate::frontmatter;
use crate::model::{ImportBody, PreparedBundle, Requirements, SupportManifest};

const SNAPSHOT_PREFIX: &str = "fleet-bundles/sha256/";
const MAX_CREDENTIALS: usize = 32;
const MAX_TOOLS: usize = 64;
const MAX_HOSTS: usize = 64;
const MAX_NAME_LEN: usize = 200;
const MAX_HOST_LEN: usize = 253;

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
    let too_many = credentials.len() > MAX_CREDENTIALS
        || tools.len() > MAX_TOOLS
        || network_hosts.len() > MAX_HOSTS;
    let too_long = credentials
        .iter()
        .chain(&tools)
        .any(|value| value.len() > MAX_NAME_LEN)
        || network_hosts.iter().any(|value| value.len() > MAX_HOST_LEN);
    if too_many || too_long {
        return Err(InvalidBundle::RequirementsTooLarge.into());
    }
    Ok(Requirements {
        credentials,
        tools,
        network_hosts,
        support_files,
        trigger_present: true,
    })
}

fn hashes(body: &ImportBody) -> (String, Vec<SupportManifest>) {
    let mut bundle = Sha256::new();
    bundle.update(&body.skill_markdown);
    bundle.update([0]);
    if let Some(trigger) = &body.trigger_markdown {
        bundle.update(trigger);
    }
    bundle.update([0]);
    let manifest = body
        .support_files
        .iter()
        .map(|file| {
            bundle.update(file.path.as_bytes());
            bundle.update([0]);
            bundle.update(&file.content);
            bundle.update([0]);
            SupportManifest {
                path: file.path.clone(),
                size_bytes: file.content.len(),
                sha256: hex::encode(Sha256::digest(&file.content)),
            }
        })
        .collect();
    (hex::encode(bundle.finalize()), manifest)
}

#[cfg(test)]
mod tests;
