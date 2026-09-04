//! YAML frontmatter parsing through Serde rather than a bespoke scanner.

use serde::Deserialize;

use crate::error::{Error, InvalidBundle, Result};

#[derive(Debug, Deserialize)]
pub(crate) struct Skill {
    pub name: String,
    pub description: String,
    pub version: String,
}

pub(crate) fn skill(markdown: &[u8]) -> Result<Skill> {
    let parsed: Skill = parse(markdown, "SKILL.md", InvalidBundle::InvalidSkill)?;
    let valid_name = !parsed.name.is_empty()
        && parsed.name.len() <= 64
        && parsed
            .name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !parsed.name.starts_with('-')
        && !parsed.name.ends_with('-');
    let valid = valid_name
        && !parsed.description.is_empty()
        && semver::Version::parse(&parsed.version).is_ok();
    valid
        .then_some(parsed)
        .ok_or_else(|| InvalidBundle::InvalidSkill.into())
}

pub(crate) fn trigger(markdown: &[u8]) -> Result<afd_fleet_runtime::FleetConfig> {
    let parsed: serde_json::Value = parse(markdown, "TRIGGER.md", InvalidBundle::InvalidTrigger)?;
    afd_fleet_runtime::FleetConfig::authored(&parsed.to_string()).map_err(Error::TriggerConfig)
}

/// The YAML between the fences and the body after them, when the document
/// opens with a frontmatter block.
///
/// Split out so the credential rule reads the SAME block this parses rather
/// than scanning the raw bytes for key shapes — a scanner cannot tell a
/// mapping from a comment, and the one that could not is what refused the
/// first-party `platform-ops` bundle.
pub(crate) fn split(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix("---\n")?;
    rest.strip_suffix("\n---")
        .map(|yaml| (yaml, ""))
        .or_else(|| rest.split_once("\n---\n"))
        .or_else(|| rest.split_once("\n---\r\n"))
}

fn parse<T: for<'de> Deserialize<'de>>(
    markdown: &[u8],
    document: &'static str,
    missing: InvalidBundle,
) -> Result<T> {
    let text = core::str::from_utf8(markdown)
        .map_err(|source| Error::FrontmatterUtf8 { document, source })?;
    let (yaml, _body) = split(text).ok_or(missing)?;
    serde_yaml_ng::from_str(yaml).map_err(|source| Error::FrontmatterYaml { document, source })
}
