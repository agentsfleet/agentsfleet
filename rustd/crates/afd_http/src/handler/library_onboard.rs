//! The onboarding request, parsed once for both planes that accept one.
//!
//! `POST /v1/admin/fleet-libraries` and
//! `POST /v1/workspaces/{workspace_id}/fleet-libraries` take the SAME body and
//! refuse the same shapes — the Zig serves both from one `ImportRequest` and
//! one parse. What differs is where the bundle lands, which is
//! [`Destination`](afd_library::Destination)'s job and not this module's.
//!
//! It lives here rather than in either plane crate because a copy in each is
//! the failure this prevents: the two would drift, and the surface that drifted
//! would accept a body the other refuses while both document one schema.
//!
//! # The kind is PARSED, and that is what removes the unreachable arm
//!
//! A caller's `source_kind` becomes a [`SourceKind`] at this boundary or the
//! request is refused. Everything downstream matches a closed set of three,
//! so a dispatch cannot carry a "cannot happen" arm and a spelling cannot drift
//! between the surface that accepts it and the store that writes it.

use std::borrow::Cow;

use afd_core::clock::UnixMillis;
use afd_core::error_code::{self, ErrorCode};
use afd_library::{
    Destination, ImportBody, LibraryImports, Onboarded, Repository, SourceKind, valid_revision,
};
use afd_wire::admin::AdminLibraryImport;

/// What one onboarding request asks for, carrying only what its kind needs.
///
/// The point of returning this rather than a validated struct plus its kind:
/// after [`parse`] there is no `Option` left for a caller to re-check. An
/// upload's skill document is present because the variant holds a built
/// [`ImportBody`], and a github source's repository is present because the
/// variant holds it — so neither plane writes a defensive arm for a state the
/// parse already refused, and neither can forget to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Onboarding<'a> {
    /// An inline upload, already assembled.
    Upload(ImportBody),
    /// A public repository to fetch.
    Github {
        /// `owner/repo`, already shaped.
        repository: Cow<'a, str>,
        /// The branch or tag, or the importer's default.
        revision: Option<Cow<'a, str>>,
    },
    /// A first-party template, by name.
    Template(Cow<'a, str>),
}

/// The refusal an empty body earns.
pub const DETAIL_BODY_REQUIRED: &str = "A request body is required";

/// The refusal a body this daemon cannot read earns.
pub const DETAIL_MALFORMED_JSON: &str = "The request body is not valid JSON";

/// The refusal a `source_kind` outside the three this daemon serves earns.
pub const DETAIL_SOURCE_KIND: &str = "source_kind must be template, upload, or github";

/// The refusal an upload carrying no skill document earns.
pub const DETAIL_MISSING_SKILL: &str = "missing_skill";

/// The refusal an upload carrying support files earns.
pub const DETAIL_UPLOAD_ATTACHMENTS: &str =
    "upload sources cannot carry support files; use a github or template source";

/// The refusal an upload naming a repository ref earns.
pub const DETAIL_UPLOAD_REVISION: &str = "upload sources cannot carry a repository ref";

/// The refusal a template naming a repository ref earns.
pub const DETAIL_TEMPLATE_REVISION: &str = "template sources cannot carry a repository ref";

/// The refusal a malformed source reference earns.
pub const DETAIL_SOURCE_REF: &str = "source_ref must be 'owner/repo' for a github source";

/// One parsed onboarding request.
///
/// Two values rather than one because they belong to different decisions: what
/// to onboard is every plane's, and whether to overwrite a name a different
/// source owns is the platform catalogue's alone. The tenant plane binds
/// `replace_requested` and does nothing with it, which is correct — its
/// [`Destination`](afd_library::Destination) arm has no field for it, so a
/// workspace onboarding cannot act on the flag even by mistake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed<'a> {
    /// What to onboard, carrying only what its kind needs.
    pub onboarding: Onboarding<'a>,
    /// Whether the caller asked to overwrite an existing owner of the name.
    pub replace_requested: bool,
}

/// The request, with its source kind PARSED rather than checked.
///
/// The kind comes back as a [`SourceKind`] and not a string, so a caller's
/// dispatch is exhaustive over the three this daemon serves and carries no
/// unreachable arm — the spelling is `SourceKind`'s own, which is also what
/// stops a plane and the store from drifting onto different ones (RULE UFS).
///
/// # Errors
/// Refuses an empty or unreadable body, a source kind this daemon does not
/// serve, and every combination of fields a kind does not accept — each with
/// the registry code and sentence a caller reads.
pub fn parse(body: &[u8]) -> Result<Parsed<'_>, (ErrorCode, &'static str)> {
    if body.is_empty() {
        return Err((error_code::INVALID_REQUEST, DETAIL_BODY_REQUIRED));
    }
    let request = afd_core::json::object_from_slice::<AdminLibraryImport<'_>>(body)
        .map_err(|_error| (error_code::INVALID_REQUEST, DETAIL_MALFORMED_JSON))?;
    let kind = SourceKind::parse(request.source_kind.as_ref())
        .ok_or((error_code::FLEET_BUNDLE_INVALID, DETAIL_SOURCE_KIND))?;

    let refusal = match kind {
        SourceKind::Upload if request.skill_markdown.is_none() => Some(DETAIL_MISSING_SKILL),
        SourceKind::Upload if !request.support_files.is_empty() => Some(DETAIL_UPLOAD_ATTACHMENTS),
        SourceKind::Upload if request.revision.is_some() => Some(DETAIL_UPLOAD_REVISION),
        SourceKind::Template if request.revision.is_some() => Some(DETAIL_TEMPLATE_REVISION),
        SourceKind::Template if !valid_revision(request.source_ref.as_ref()) => {
            Some(DETAIL_SOURCE_REF)
        }
        SourceKind::Github
            if Repository::parse(request.source_ref.as_ref()).is_err()
                || request
                    .revision
                    .as_deref()
                    .is_some_and(|revision| !valid_revision(revision)) =>
        {
            Some(DETAIL_SOURCE_REF)
        }
        SourceKind::Upload | SourceKind::Github | SourceKind::Template => None,
    };
    if let Some(detail) = refusal {
        return Err((error_code::FLEET_BUNDLE_INVALID, detail));
    }
    let replace_requested = request.replace;
    let onboarding = match kind {
        // The skill is unwrapped HERE and nowhere else: the arm above refused
        // an upload without one, so this is the single place that knows the
        // refusal has already happened.
        SourceKind::Upload => Onboarding::Upload(ImportBody {
            source_kind: SourceKind::Upload,
            source_ref: request.source_ref.into_owned(),
            source_revision: None,
            skill_markdown: request
                .skill_markdown
                .map(|skill| skill.into_owned().into_bytes())
                .unwrap_or_default(),
            trigger_markdown: request
                .trigger_markdown
                .map(|markdown| markdown.into_owned().into_bytes()),
            support_files: Vec::new(),
        }),
        SourceKind::Github => Onboarding::Github {
            repository: request.source_ref,
            revision: request.revision,
        },
        SourceKind::Template => Onboarding::Template(request.source_ref),
    };
    Ok(Parsed {
        onboarding,
        replace_requested,
    })
}

/// Runs one parsed onboarding into `into`.
///
/// The other half of [`parse`], and shared for the same reason: the mapping
/// from a variant to the verb that serves it was written identically in both
/// planes, so a fourth source kind would have had to be added to both matches
/// and would compile if only one was.
///
/// # Errors
/// Reports every way a source can fail to yield a bundle — a fetch that did not
/// answer, an archive that would not decode, a bundle that did not validate —
/// and the catalogue write behind it.
pub async fn run(
    imports: &LibraryImports,
    onboarding: Onboarding<'_>,
    into: Destination<'_>,
    now: UnixMillis,
) -> afd_library::Result<Onboarded> {
    match onboarding {
        Onboarding::Upload(body) => imports.upload(&body, into, now).await,
        Onboarding::Github {
            repository,
            revision,
        } => {
            imports
                .github(repository.as_ref(), revision.as_deref(), into, now)
                .await
        }
        Onboarding::Template(template) => imports.template(template.as_ref(), into, now).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_well_formed_upload_comes_back_with_its_kind_parsed() {
        // The kind is a value downstream matches on, not a string it re-checks.
        assert_eq!(
            parse(br#"{"source_kind":"upload","skill_markdown":"---"}"#)
                .map(|parsed| matches!(parsed.onboarding, Onboarding::Upload(_))),
            Ok(true)
        );
    }

    #[test]
    fn every_shape_a_kind_does_not_accept_is_refused_before_any_io() {
        // The kind comes back PARSED, so a caller downstream matches on a
        // closed set rather than on a string it has to re-check.
        assert_eq!(
            parse(br#"{"source_kind":"upload","skill_markdown":"---"}"#)
                .map(|parsed| matches!(parsed.onboarding, Onboarding::Upload(_))),
            Ok(true)
        );
        assert_eq!(
            parse(br#"{"source_kind":"upload","skill_markdown":"---","ref":"main"}"#)
                .map(|_parsed| ()),
            Err((error_code::FLEET_BUNDLE_INVALID, DETAIL_UPLOAD_REVISION))
        );
        assert_eq!(
            parse(br#"{"source_kind":"github","source_ref":"owner/repo/extra"}"#).map(|_parsed| ()),
            Err((error_code::FLEET_BUNDLE_INVALID, DETAIL_SOURCE_REF))
        );

        for (body, expected) in [
            (b"".as_slice(), DETAIL_BODY_REQUIRED),
            (b"[]".as_slice(), DETAIL_MALFORMED_JSON),
            (br#"{"source_kind":"upload"}"#, DETAIL_MISSING_SKILL),
            // Carries the skill so the missing-skill guard above passes and the
            // attachment arm is the one that answers. Without it this case
            // silently graded the wrong refusal.
            (
                br#"{"source_kind":"upload","skill_markdown":"---","support_files":[{}]}"#,
                DETAIL_UPLOAD_ATTACHMENTS,
            ),
            (
                br#"{"source_kind":"template","source_ref":"reviewer","ref":"main"}"#,
                DETAIL_TEMPLATE_REVISION,
            ),
            (
                br#"{"source_kind":"template","source_ref":"bad/ref"}"#,
                DETAIL_SOURCE_REF,
            ),
            (
                br#"{"source_kind":"github","source_ref":"owner/repo","ref":"bad/ref"}"#,
                DETAIL_SOURCE_REF,
            ),
            (br#"{"source_kind":"unknown"}"#, DETAIL_SOURCE_KIND),
        ] {
            assert_eq!(
                parse(body).map(|_parsed| ()),
                Err((
                    if expected == DETAIL_BODY_REQUIRED || expected == DETAIL_MALFORMED_JSON {
                        error_code::INVALID_REQUEST
                    } else {
                        error_code::FLEET_BUNDLE_INVALID
                    },
                    expected,
                ))
            );
        }
    }
}
