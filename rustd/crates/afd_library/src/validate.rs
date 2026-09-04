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

/// Byte shapes whose PRESENCE is the finding: each is a credential value, not
/// a key that might be followed by one.
const VALUE_MARKERS: [&[u8]; 2] = [b"op://", b"BEGIN PRIVATE KEY"];

/// Keys that are only a finding when a REAL value follows them.
///
/// The key alone proves nothing. A bundle is allowed to document the shape of
/// the credential it needs — `tests/fixtures/fleetbundle/platform-ops`
/// does exactly that, in a YAML comment — and refusing that is refusing
/// documentation, not a leak.
const ASSIGNMENT_MARKERS: [&[u8]; 4] = [
    b"api_key:",
    b"access_token:",
    b"client_secret:",
    b"webhook_secret:",
];

/// Openers of a value that stands in for a credential instead of being one:
/// `<gh PAT>` documents a shape, `{{model}}` is a template hole, and
/// `${secrets.NAME.FIELD}` is the substitution reference the tool bridge
/// resolves AFTER the sandbox closes. None is a secret.
const PLACEHOLDER_OPENERS: [&str; 2] = ["{{", "${"];

/// Where a value token ends, inside an inline map or at end of line.
const VALUE_TERMINATORS: [u8; 4] = *b",}\n\r";

/// Trimmed from a value before it is judged: YAML quoting and padding. The
/// quotes come off FIRST, or a quoted placeholder reads as an opaque value.
const VALUE_TRIM: [char; 4] = [' ', '\t', '"', '\''];

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

/// Whether the bytes carry a credential VALUE.
///
/// A self-evidencing marker is a finding wherever it appears. An assignment
/// marker is a finding only when a real value follows it: the same key
/// followed by `"<base64>"`, `{{model}}` or `${secrets.github.token}` is a
/// bundle documenting what it needs, and the scanner used to refuse the
/// first-party `platform-ops` bundle for exactly that.
fn contains_credential(content: &[u8]) -> bool {
    if VALUE_MARKERS.iter().any(|marker| contains(content, marker)) {
        return true;
    }
    ASSIGNMENT_MARKERS
        .iter()
        .any(|marker| assigns_real_value(content, marker))
}

/// Whether `marker` occurs anywhere in `content`.
fn contains(content: &[u8], marker: &[u8]) -> bool {
    positions(content, marker).next().is_some()
}

/// Every offset just past an occurrence of `marker`.
fn positions<'a>(content: &'a [u8], marker: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
    content
        .windows(marker.len())
        .enumerate()
        .filter_map(move |(at, window)| (window == marker).then_some(at + marker.len()))
}

/// Whether any occurrence of `marker` is followed by something that is a
/// credential rather than a stand-in for one.
fn assigns_real_value(content: &[u8], marker: &[u8]) -> bool {
    positions(content, marker).any(|from| {
        content.get(from..).is_some_and(|rest| {
            let end = rest
                .iter()
                .position(|byte| VALUE_TERMINATORS.contains(byte))
                .unwrap_or(rest.len());
            rest.get(..end).is_some_and(is_real_value)
        })
    })
}

/// Whether a value token is a credential rather than a placeholder.
fn is_real_value(raw: &[u8]) -> bool {
    let Ok(text) = core::str::from_utf8(raw) else {
        // Bytes that are not text are not a documented shape either.
        return true;
    };
    let value = text.trim_matches(VALUE_TRIM);
    if value.is_empty() {
        return false;
    }
    if PLACEHOLDER_OPENERS
        .iter()
        .any(|opener| value.starts_with(opener))
    {
        return false;
    }
    // `<gh PAT>` and friends: a shape in angle brackets, never a value.
    !(value.starts_with('<') && value.ends_with('>'))
}

#[cfg(test)]
#[path = "validate/tests.rs"]
mod tests;
