//! Bounds and path safety for untrusted bundle bytes.

use std::path::{Component, Path};

use crate::error::InvalidBundle;
use crate::model::{ImportBody, SupportFile};

const MAX_SOURCE_REF_LEN: usize = 512;
const MAX_MARKDOWN_LEN: usize = 200 * 1024;
const MAX_SUPPORT_FILES: usize = 32;
const MAX_SUPPORT_PATH_LEN: usize = 160;
const MAX_SUPPORT_FILE_LEN: usize = 64 * 1024;
const MAX_SUPPORT_TOTAL_LEN: usize = 256 * 1024;
const CREDENTIAL_MARKERS: [&[u8]; 6] = [
    b"op://",
    b"BEGIN PRIVATE KEY",
    b"api_key:",
    b"access_token:",
    b"client_secret:",
    b"webhook_secret:",
];

pub(crate) fn body(body: &ImportBody) -> Result<(), InvalidBundle> {
    if body.source_ref.len() > MAX_SOURCE_REF_LEN {
        return Err(InvalidBundle::SourceRefTooLong);
    }
    if body.skill_markdown.is_empty() {
        return Err(InvalidBundle::MissingSkill);
    }
    if body.skill_markdown.len() > MAX_MARKDOWN_LEN {
        return Err(InvalidBundle::SkillTooLarge);
    }
    if body.trigger_markdown.as_ref().is_some_and(|value| value.is_empty() || value.len() > MAX_MARKDOWN_LEN) {
        return Err(InvalidBundle::TriggerTooLarge);
    }
    support_files(&body.support_files)
}

fn support_files(files: &[SupportFile]) -> Result<(), InvalidBundle> {
    if files.len() > MAX_SUPPORT_FILES {
        return Err(InvalidBundle::TooManySupportFiles);
    }
    let mut total = 0usize;
    for file in files {
        validate_path(&file.path)?;
        if file.content.len() > MAX_SUPPORT_FILE_LEN {
            return Err(InvalidBundle::SupportFileTooLarge);
        }
        total = total.saturating_add(file.content.len());
        if total > MAX_SUPPORT_TOTAL_LEN {
            return Err(InvalidBundle::SupportFilesTooLarge);
        }
        if CREDENTIAL_MARKERS.iter().any(|marker| contains(&file.content, marker)) {
            return Err(InvalidBundle::EmbeddedCredential);
        }
    }
    Ok(())
}

fn validate_path(raw: &str) -> Result<(), InvalidBundle> {
    let invalid_text = raw.is_empty()
        || raw.len() > MAX_SUPPORT_PATH_LEN
        || raw.contains('\\')
        || raw.split('/').any(|part| part.is_empty())
        || matches!(raw, "SKILL.md" | "TRIGGER.md");
    let invalid_component = Path::new(raw)
        .components()
        .any(|part| !matches!(part, Component::Normal(_)));
    if invalid_text || invalid_component {
        Err(InvalidBundle::UnsafeSupportPath)
    } else {
        Ok(())
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}
