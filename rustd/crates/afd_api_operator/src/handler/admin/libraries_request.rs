//! Turning an admin catalogue PATCH body into the [`LibraryPatch`] the store takes.
//!
//! Split from [`super::libraries`] along the line the file cap and the tests
//! both want: everything here is total, synchronous and datastore-free, so the
//! refusal surface in front of the write is proven without driving HTTP. Each
//! bound below is one the store would otherwise learn from a database error,
//! which is too late to name the field that was wrong.

use std::borrow::Cow;

use afd_core::error_code;
use afd_library::{LibraryPatch, Repository, valid_revision};
use afd_validate::Sentences;
use afd_wire::admin::{
    AdminLibraryPatch, REASON_CREDENTIAL_MAX_BYTES, REASON_MAX_BYTES, REASONS_MAX,
};
use const_format::concatcp;
use garde::Validate as _;

const DETAIL_BODY_REQUIRED: &str = "A request body is required";

const DETAIL_MALFORMED_JSON: &str = "The request body is not valid JSON";

const DETAIL_NAME_INVALID: &str = "A name is required, and must be at most 200 characters";

const DETAIL_REPO_INVALID: &str =
    "A repository must be owner/repo, using letters, digits, '.', '-' or '_'";

const DETAIL_REF_INVALID: &str =
    "A ref must be a branch or tag name, using letters, digits, '.', '-' or '_'";

const DETAIL_REASONS_INVALID: &str =
    "required_credentials_reasons must be an object mapping credential names to strings";

/// The refusal reason copy past the install gate's caps earns.
///
/// One sentence naming all three caps: they are one rule on the wire type,
/// and a caller who broke one fixes it by reading the numbers.
const DETAIL_REASONS_BOUNDS: &str = concatcp!(
    "required_credentials_reasons may carry at most ",
    REASONS_MAX,
    " entries, each name at most ",
    REASON_CREDENTIAL_MAX_BYTES,
    " bytes and each reason at most ",
    REASON_MAX_BYTES,
    " bytes"
);

/// The path `garde` reports a name-length break under.
const FIELD_NAME: &str = "name";

/// The path `garde` reports a reason-copy break under.
const FIELD_REASONS: &str = "required_credentials_reasons";

/// The sentence each bound on [`AdminLibraryPatch`] earns. The name is listed
/// first, the order the two were refused in before the caps moved onto the
/// wire type.
const BOUNDS: Sentences = Sentences::new(
    &[
        (FIELD_NAME, DETAIL_NAME_INVALID),
        (FIELD_REASONS, DETAIL_REASONS_BOUNDS),
    ],
    DETAIL_NAME_INVALID,
);

pub(super) fn patch_request(
    body: &[u8],
) -> Result<LibraryPatch, (error_code::ErrorCode, &'static str)> {
    if body.is_empty() {
        return Err((error_code::INVALID_REQUEST, DETAIL_BODY_REQUIRED));
    }
    let request = afd_http::handler::read_body::<AdminLibraryPatch<'_>>(body)
        .map_err(|_error| (error_code::INVALID_REQUEST, DETAIL_MALFORMED_JSON))?;
    request
        .validate()
        .map_err(|report| (error_code::INVALID_REQUEST, BOUNDS.pick(&report)))?;
    if request
        .source_repo
        .as_deref()
        .is_some_and(|repo| Repository::parse(repo).is_err())
    {
        return Err((error_code::FLEET_BUNDLE_INVALID, DETAIL_REPO_INVALID));
    }
    if request
        .source_ref
        .as_deref()
        .is_some_and(|revision| !valid_revision(revision))
    {
        return Err((error_code::FLEET_BUNDLE_INVALID, DETAIL_REF_INVALID));
    }
    if !reasons_shaped(request.required_credentials_reasons.as_ref()) {
        return Err((error_code::INVALID_REQUEST, DETAIL_REASONS_INVALID));
    }
    Ok(LibraryPatch::new(
        request.name.map(Cow::into_owned),
        request.description.map(Cow::into_owned),
        request.source_repo.map(Cow::into_owned),
        request.source_ref.map(Cow::into_owned),
        request.required_credentials_reasons,
        request.published,
    ))
}

/// Whether the reason copy, when sent, is an object mapping names to strings.
///
/// Its SHAPE, which no bound can judge; the caps on it are the wire type's
/// custom rule, proved by `validate` above.
fn reasons_shaped(reasons: Option<&serde_json::Value>) -> bool {
    reasons.is_none_or(|reasons| {
        reasons
            .as_object()
            .is_some_and(|reasons| reasons.values().all(serde_json::Value::is_string))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_validation_covers_identity_and_reason_bounds() {
        assert_eq!(
            patch_request(br#"{"description":"new"}"#).map(|_patch| ()),
            Ok(())
        );
        assert_eq!(
            patch_request(b""),
            Err((error_code::INVALID_REQUEST, DETAIL_BODY_REQUIRED))
        );
        assert_eq!(
            patch_request(br#"{"source_repo":"owner/repo/extra"}"#),
            Err((error_code::FLEET_BUNDLE_INVALID, DETAIL_REPO_INVALID))
        );
        assert_eq!(
            patch_request(br#"{"source_ref":"../main"}"#),
            Err((error_code::FLEET_BUNDLE_INVALID, DETAIL_REF_INVALID))
        );
        assert_eq!(
            patch_request(br#"{"required_credentials_reasons":[]}"#),
            Err((error_code::INVALID_REQUEST, DETAIL_REASONS_INVALID))
        );
        assert_eq!(
            patch_request(br#"{"required_credentials_reasons":{"github":42}}"#),
            Err((error_code::INVALID_REQUEST, DETAIL_REASONS_INVALID))
        );
    }

    /// A reason that IS a string is then held to its byte caps, on both the
    /// credential name and the sentence — and one inside both is kept.
    #[test]
    fn a_string_reason_is_bounded_on_both_its_name_and_its_sentence() {
        assert_eq!(
            patch_request(br#"{"required_credentials_reasons":{"github":"opens the release PR"}}"#)
                .map(|_patch| ()),
            Ok(())
        );

        let sentence_past_cap = format!(
            r#"{{"required_credentials_reasons":{{"github":"{}"}}}}"#,
            "r".repeat(REASON_MAX_BYTES + 1)
        );
        assert_eq!(
            patch_request(sentence_past_cap.as_bytes()),
            Err((error_code::INVALID_REQUEST, DETAIL_REASONS_BOUNDS))
        );

        let name_past_cap = format!(
            r#"{{"required_credentials_reasons":{{"{}":"fine"}}}}"#,
            "c".repeat(REASON_CREDENTIAL_MAX_BYTES + 1)
        );
        assert_eq!(
            patch_request(name_past_cap.as_bytes()),
            Err((error_code::INVALID_REQUEST, DETAIL_REASONS_BOUNDS))
        );
    }

    /// One entry past the most a library may explain earns the same sentence.
    #[test]
    fn reasons_past_the_entry_cap_are_refused() {
        let entries: Vec<String> = (0..=REASONS_MAX)
            .map(|index| format!(r#""c{index}":"why""#))
            .collect();
        let body = format!(
            r#"{{"required_credentials_reasons":{{{}}}}}"#,
            entries.join(",")
        );
        assert_eq!(
            patch_request(body.as_bytes()),
            Err((error_code::INVALID_REQUEST, DETAIL_REASONS_BOUNDS))
        );
    }
}
