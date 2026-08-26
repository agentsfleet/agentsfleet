//! YAML frontmatter parsing through Serde rather than a bespoke scanner.

use serde::Deserialize;

use crate::error::InvalidBundle;

#[derive(Debug, Deserialize)]
pub(crate) struct Skill {
    pub name: String,
    pub description: String,
    pub version: String,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct Runtime {
    #[serde(default)]
    pub credentials: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub network: Network,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct Network {
    #[serde(default)]
    pub allow: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Trigger {
    pub name: String,
    #[serde(rename = "x-agentsfleet")]
    pub runtime: Runtime,
}

pub(crate) fn skill(markdown: &[u8]) -> Result<Skill, InvalidBundle> {
    let parsed: Skill = parse(markdown).map_err(|()| InvalidBundle::InvalidSkill)?;
    let valid_name = !parsed.name.is_empty()
        && parsed.name.len() <= 64
        && parsed.name.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !parsed.name.starts_with('-')
        && !parsed.name.ends_with('-');
    let valid = valid_name
        && !parsed.description.is_empty()
        && semver::Version::parse(&parsed.version).is_ok();
    valid.then_some(parsed).ok_or(InvalidBundle::InvalidSkill)
}

pub(crate) fn trigger(markdown: &[u8]) -> Result<Trigger, InvalidBundle> {
    parse(markdown).map_err(|()| InvalidBundle::InvalidTrigger)
}

fn parse<T: for<'de> Deserialize<'de>>(markdown: &[u8]) -> Result<T, ()> {
    let text = core::str::from_utf8(markdown).map_err(|_| ())?;
    let rest = text.strip_prefix("---\n").ok_or(())?;
    let (yaml, _) = rest.split_once("\n---").ok_or(())?;
    serde_yaml_ng::from_str(yaml).map_err(|_| ())
}
