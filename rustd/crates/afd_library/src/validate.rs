//! Bounds and path safety for untrusted bundle bytes.
//!
//! The bounds are DECLARED on the types they guard, with `garde` — the
//! workspace's validation crate, already driving
//! `afd_fleet_runtime::config::raw`. What stays here is the part a derive
//! cannot express: the three rules that are predicates rather than bounds, and
//! the translation from garde's report back to [`InvalidBundle`].
//!
//! That translation is not ceremony. [`crate::Error::code`] answers
//! `UZ-REQ-002` for six variants and `UZ-BUNDLE-001` for the rest, and the two
//! ends of a single range fall on opposite sides of that line — an empty
//! `SKILL.md` is malformed, an oversized one is too large. A report carries a
//! path and a message, not a variant, so each rule that owns a distinct variant
//! emits a stable code and is mapped back here.

#![expect(
    clippy::trivially_copy_pass_by_ref,
    clippy::ref_option,
    reason = "garde fixes the custom-validator signature at `fn(&T, &C) -> Result`. The `()` context and the `Option` field arrive by reference because the derive passes them that way; a validator spelled the way clippy prefers is not callable from the attribute that runs it."
)]

use std::path::{Component, Path};

use crate::error::InvalidBundle;
use crate::model::ImportBody;

pub(crate) const MAX_SOURCE_REF_LEN: usize = 512;
pub(crate) const MAX_MARKDOWN_LEN: usize = 200 * 1024;
pub(crate) const MAX_SUPPORT_FILES: usize = 32;
const MAX_SUPPORT_PATH_LEN: usize = 160;
pub(crate) const MAX_SUPPORT_FILE_LEN: usize = 64 * 1024;
const MAX_SUPPORT_TOTAL_LEN: usize = 256 * 1024;

/// Byte shapes whose PRESENCE is the finding: each IS a credential, wherever
/// it appears, so no structure has to be understood to judge one.
const VALUE_MARKERS: [&[u8]; 2] = [b"op://", b"BEGIN PRIVATE KEY"];

/// Keys whose VALUE is the finding.
///
/// Judged against the parsed document, never the raw bytes. A key is only a
/// leak when a mapping assigns it something real, and only a parser can tell
/// that from a comment describing the shape the bundle needs — which is what
/// `tests/fixtures/fleetbundle/platform-ops` does, and what a byte scanner
/// refused it for.
const CREDENTIAL_KEYS: [&str; 4] = ["api_key", "access_token", "client_secret", "webhook_secret"];

/// Openers of a value that stands in for a credential instead of being one:
/// `{{model}}` is a template hole and `${secrets.NAME.FIELD}` is the
/// substitution reference the tool bridge resolves AFTER the sandbox closes.
const PLACEHOLDER_OPENERS: [&str; 2] = ["{{", "${"];

const CODE_MISSING_SKILL: &str = "bundle.skill.missing";
const CODE_TRIGGER_EMPTY: &str = "bundle.trigger.empty";
const CODE_UNSAFE_PATH: &str = "bundle.support.path.unsafe";
const CODE_SUPPORT_TOTAL: &str = "bundle.support.bytes.total";
const CODE_EMBEDDED_CREDENTIAL: &str = "bundle.credential.embedded";

const PATH_SOURCE_REF: &str = "source_ref";
const PATH_SKILL: &str = "skill_markdown";
const PATH_TRIGGER: &str = "trigger_markdown";
const PATH_SUPPORT_FILES: &str = "support_files";
const SEGMENT_CONTENT: &str = "content";

const ROOT_DOCUMENTS: [&str; 2] = ["SKILL.md", "TRIGGER.md"];

/// Proves every declared bound and predicate over one untrusted body.
///
/// # Errors
/// The [`InvalidBundle`] rule the body broke first, in field-declaration order.
pub(crate) fn body(value: &ImportBody) -> Result<(), InvalidBundle> {
    use garde::Validate as _;

    value.validate().map_err(|report| {
        report
            .iter()
            .next()
            .map_or(InvalidBundle::MissingSkill, |(path, error)| {
                classify(&path.to_string(), &error.to_string())
            })
    })
}

/// Maps one reported violation back to the variant its classification rides on.
///
/// A custom rule names itself, so it is matched first; what remains is a
/// `length` bound, and the path alone identifies which.
fn classify(path: &str, message: &str) -> InvalidBundle {
    match message {
        CODE_MISSING_SKILL => return InvalidBundle::MissingSkill,
        CODE_TRIGGER_EMPTY => return InvalidBundle::InvalidTrigger,
        CODE_UNSAFE_PATH => return InvalidBundle::UnsafeSupportPath,
        CODE_SUPPORT_TOTAL => return InvalidBundle::SupportFilesTooLarge,
        CODE_EMBEDDED_CREDENTIAL => return InvalidBundle::EmbeddedCredential,
        _ => {}
    }
    if path == PATH_SOURCE_REF {
        InvalidBundle::SourceRefTooLong
    } else if path == PATH_SKILL {
        InvalidBundle::SkillTooLarge
    } else if path == PATH_TRIGGER {
        InvalidBundle::TriggerTooLarge
    } else if path == PATH_SUPPORT_FILES {
        InvalidBundle::TooManySupportFiles
    } else if path.starts_with(PATH_SUPPORT_FILES) && path.ends_with(SEGMENT_CONTENT) {
        InvalidBundle::SupportFileTooLarge
    } else {
        InvalidBundle::UnsafeSupportPath
    }
}

/// A bundle without a skill document is not a bundle.
///
/// Separate from the `length` bound on the same field because an absent
/// document and an oversized one answer different public codes.
///
/// # Errors
/// [`CODE_MISSING_SKILL`] when no bytes were supplied.
pub(crate) fn skill_present(value: &[u8], _: &()) -> garde::Result {
    if value.is_empty() {
        return Err(garde::Error::new(CODE_MISSING_SKILL));
    }
    Ok(())
}

/// Absent is legal; present-and-empty is a malformed document.
///
/// # Errors
/// [`CODE_TRIGGER_EMPTY`] when the document is supplied and carries no bytes.
pub(crate) fn trigger_non_empty(value: &Option<Vec<u8>>, _: &()) -> garde::Result {
    if value.as_ref().is_some_and(Vec::is_empty) {
        return Err(garde::Error::new(CODE_TRIGGER_EMPTY));
    }
    Ok(())
}

/// Refuses a support path that could escape the bundle or shadow a root
/// document.
///
/// # Errors
/// [`CODE_UNSAFE_PATH`] for an empty, over-long, absolute, traversing,
/// backslash-bearing, or root-colliding path.
pub(crate) fn safe_path(raw: &str, _: &()) -> garde::Result {
    let invalid_text = raw.is_empty()
        || raw.len() > MAX_SUPPORT_PATH_LEN
        || raw.contains('\\')
        || raw.split('/').any(str::is_empty)
        || ROOT_DOCUMENTS.contains(&raw);
    let invalid_component = Path::new(raw)
        .components()
        .any(|part| !matches!(part, Component::Normal(_)));
    if invalid_text || invalid_component {
        return Err(garde::Error::new(CODE_UNSAFE_PATH));
    }
    Ok(())
}

/// Refuses credential VALUES in a support file's bytes.
///
/// # Errors
/// [`CODE_EMBEDDED_CREDENTIAL`] when the content carries one.
pub(crate) fn no_credential_bytes(value: &[u8], _: &()) -> garde::Result {
    if contains_credential(value) {
        return Err(garde::Error::new(CODE_EMBEDDED_CREDENTIAL));
    }
    Ok(())
}

/// The two rules that read the whole body rather than one field: the aggregate
/// support-byte cap, and the credential scan over the root documents.
///
/// # Errors
/// [`CODE_EMBEDDED_CREDENTIAL`] or [`CODE_SUPPORT_TOTAL`], whichever the body
/// broke.
pub(crate) fn aggregate(value: &ImportBody, _: &()) -> garde::Result {
    if contains_credential(&value.skill_markdown)
        || value
            .trigger_markdown
            .as_deref()
            .is_some_and(contains_credential)
    {
        return Err(garde::Error::new(CODE_EMBEDDED_CREDENTIAL));
    }
    let total: usize = value
        .support_files
        .iter()
        .map(|file| file.content.len())
        .sum();
    if total > MAX_SUPPORT_TOTAL_LEN {
        return Err(garde::Error::new(CODE_SUPPORT_TOTAL));
    }
    Ok(())
}

/// Whether the bytes carry a credential.
///
/// Two rules, because the evidence differs. A value marker is self-evidencing
/// and is looked for in the bytes. A credential KEY proves nothing on its own,
/// so it is judged where it has meaning: in the parsed document, where a
/// comment is not a mapping and a documented shape is not an assignment.
fn contains_credential(content: &[u8]) -> bool {
    if VALUE_MARKERS.iter().any(|marker| {
        content
            .windows(marker.len())
            .any(|window| window == *marker)
    }) {
        return true;
    }
    assigns_credential_anywhere(content)
}

/// Whether any document these bytes carry assigns a credential.
///
/// BOTH halves of a fenced file are read — the frontmatter block and the body
/// after it — because a credential pasted below the closing fence is in the
/// stored bundle exactly as much as one above it. An unfenced file is read
/// whole, so a support file that is plain configuration is judged like a root
/// document's frontmatter.
///
/// Bytes that will not parse assign nothing. Whether a document is well-formed
/// is `InvalidSkill`/`InvalidTrigger`'s question, not this rule's, and
/// answering it here would refuse a malformed bundle for carrying a secret it
/// does not.
fn assigns_credential_anywhere(content: &[u8]) -> bool {
    let Ok(text) = core::str::from_utf8(content) else {
        return false;
    };
    let (block, body) = crate::frontmatter::split(text).unwrap_or((text, ""));
    [block, body].into_iter().any(|source| {
        serde_yaml_ng::from_str::<serde_yaml_ng::Value>(source)
            .is_ok_and(|value| assigns_credential(&value))
    })
}

/// Whether any credential key in the document is assigned a real value.
fn assigns_credential(value: &serde_yaml_ng::Value) -> bool {
    match value {
        serde_yaml_ng::Value::Mapping(mapping) => mapping.iter().any(|(key, child)| {
            key.as_str()
                .is_some_and(|name| CREDENTIAL_KEYS.contains(&name) && is_real_value(child))
                || assigns_credential(child)
        }),
        serde_yaml_ng::Value::Sequence(items) => items.iter().any(assigns_credential),
        _ => false,
    }
}

/// Whether an assigned value is a credential rather than a stand-in for one.
///
/// Only a string can be one. A nested mapping under a credential key is
/// structure, and the walk descends into it rather than judging it here.
fn is_real_value(value: &serde_yaml_ng::Value) -> bool {
    value
        .as_str()
        .is_some_and(|text| !is_placeholder(text.trim()))
}

/// Whether a value stands in for a credential instead of being one.
fn is_placeholder(value: &str) -> bool {
    value.is_empty()
        || PLACEHOLDER_OPENERS
            .iter()
            .any(|opener| value.starts_with(opener))
        // `<gh PAT>` and friends: a shape in angle brackets, never a value.
        || (value.starts_with('<') && value.ends_with('>'))
}

#[cfg(test)]
#[path = "validate/tests.rs"]
mod tests;
