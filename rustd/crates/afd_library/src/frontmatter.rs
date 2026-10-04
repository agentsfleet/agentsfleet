//! YAML frontmatter parsing through Serde rather than a bespoke scanner.

use afd_validate::charset;
use garde::{Unvalidated, Valid};
use serde::Deserialize;

use crate::error::{Error, ErrorKind, InvalidBundle, Result};

/// Longest skill name, the bound a fleet name holds downstream. The platform
/// catalogue is keyed by this name, so it bounds a catalogue id too.
pub const MAX_SKILL_NAME_LEN: usize = 64;
/// The one punctuation a skill name may carry, never at either end.
const NAME_HYPHEN: char = '-';
/// What a name opening or closing on a hyphen reports.
const REASON_EDGE_HYPHEN: &str = "must not start or end with a hyphen";

/// A SKILL.md's frontmatter, with the bounds each field must hold before the
/// version is parsed.
#[derive(Debug, Deserialize, garde::Validate)]
pub(crate) struct Skill {
    #[garde(
        length(bytes, min = 1, max = MAX_SKILL_NAME_LEN),
        custom(charset(is_name_char)),
        custom(hyphen_free_edges)
    )]
    pub name: String,
    #[garde(length(min = 1))]
    pub description: String,
    /// Read by [`afd_fleet_runtime::Version::parse`], which takes only a
    /// skill whose bounds held.
    #[garde(skip)]
    pub version: String,
}

pub(crate) fn skill(markdown: &[u8]) -> Result<Skill> {
    let parsed: Skill = parse(markdown, "SKILL.md", InvalidBundle::InvalidSkill)?;
    Unvalidated::new(parsed)
        .validate()
        .ok()
        // Only a skill whose bounds held has its version parsed.
        .filter(|skill| afd_fleet_runtime::Version::parse(&skill.version).is_ok())
        .map(Valid::into_inner)
        .ok_or_else(|| InvalidBundle::InvalidSkill.into())
}

/// Whether `character` may appear in a skill name: a kebab slug.
const fn is_name_char(character: char) -> bool {
    character.is_ascii_lowercase() || character.is_ascii_digit() || character == NAME_HYPHEN
}

/// Refuses a name that opens or closes on a hyphen.
fn hyphen_free_edges<C: ?Sized>(value: &str, _context: &C) -> garde::Result {
    if value.starts_with(NAME_HYPHEN) || value.ends_with(NAME_HYPHEN) {
        Err(garde::Error::new(REASON_EDGE_HYPHEN))
    } else {
        Ok(())
    }
}

pub(crate) fn trigger(markdown: &[u8]) -> Result<afd_fleet_runtime::FleetConfig> {
    let parsed: serde_json::Value = parse(markdown, "TRIGGER.md", InvalidBundle::InvalidTrigger)?;
    afd_fleet_runtime::FleetConfig::authored(&parsed.to_string())
        .map_err(|source| Error::from(ErrorKind::TriggerConfig { source }))
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
        .map_err(|source| Error::from(ErrorKind::FrontmatterUtf8 { document, source }))?;
    let (yaml, _body) = split(text).ok_or(missing)?;
    serde_yaml_ng::from_str(yaml)
        .map_err(|source| Error::from(ErrorKind::FrontmatterYaml { document, source }))
}
