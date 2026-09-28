//! What a client is told about a Fleet Bundle and the platform library it is
//! curated in. Split from `fleet.rs` at the length cap, in the same place in
//! `REGISTRY` order: this family follows the fleet's.

use super::Problem;
use crate::error_code;

/// This family's entries, in `REGISTRY` order.
pub(super) const BUNDLE: &[Problem] = &[
    Problem {
        code: error_code::FLEET_BUNDLE_INVALID,
        status: 400,
        title: "Invalid Fleet Bundle",
        hint: "The supplied Fleet Bundle is missing `SKILL.md` or contains unsafe, oversized, or malformed files.",
        user_message: Some(
            "That Fleet Bundle isn't valid. It's missing `SKILL.md`, or has an unsafe or oversized file. Check the source and try again.",
        ),
    },
    Problem {
        code: error_code::FLEET_BUNDLE_CREDENTIAL_NAME_INVALID,
        status: 400,
        title: "Invalid credential reference",
        hint: "A credential named in TRIGGER.md is not a storable vault key; the reference, and the rule it broke, are on the error's source.",
        // The one bundle refusal an author fixes by editing a name rather than
        // by re-packaging, so it says which characters are allowed instead of
        // sending them to look for a file that is not missing.
        user_message: Some(
            "A credential name in that Fleet Bundle isn't valid. Credential names may use only letters, digits and `_` — rename it in TRIGGER.md and try again.",
        ),
    },
    Problem {
        code: error_code::FLEET_BUNDLE_NOT_FOUND,
        status: 404,
        title: "Fleet Bundle not found",
        hint: "No installable library entry or stored snapshot matches the request in this workspace.",
        user_message: Some(
            "We couldn't find that Fleet Bundle. It may not be installed in this workspace yet — check the Fleet library.",
        ),
    },
    Problem {
        code: error_code::FLEET_BUNDLE_SECRETS_MISSING,
        status: 424,
        title: "Fleet Bundle secrets missing",
        hint: "Add the missing workspace secrets before installing this Fleet Bundle.",
        user_message: Some(
            "This Fleet Bundle needs secrets this workspace doesn't have yet. Add the missing secrets, then install again.",
        ),
    },
    Problem {
        code: error_code::FLEET_BUNDLE_FETCH_FAILED,
        status: 502,
        title: "Fleet Bundle fetch failed",
        hint: "The Fleet Bundle source could not be fetched from GitHub. The repository may be missing or private, or GitHub may be unreachable. Verify the source reference and retry.",
        user_message: Some(
            "We couldn't fetch that Fleet Bundle from GitHub. Check the source and try again.",
        ),
    },
    Problem {
        code: error_code::FLEET_BUNDLE_STORAGE_UNAVAILABLE,
        status: 503,
        title: "Fleet Bundle storage unavailable",
        hint: "Snapshot storage is not configured or is unavailable, so the validated bundle could not be stored. Retry later or contact the operator.",
        user_message: Some("We couldn't store your Fleet Bundle right now. Try again shortly."),
    },
    Problem {
        code: error_code::CATALOG_NOT_FOUND,
        status: 404,
        title: "Fleet library entry not found",
        hint: "No catalog entry matches this id. It may already be deleted — refresh the catalog.",
        user_message: Some(
            "We couldn't find that fleet. It may have already been removed — refresh the page.",
        ),
    },
    Problem {
        code: error_code::CATALOG_PUBLISH_WITHOUT_BUNDLE,
        status: 409,
        title: "Cannot publish a fleet with no bundle",
        hint: "No bundle has been fetched for this entry, so there is nothing to publish. Fetch it from its repository first.",
        user_message: Some(
            "There's no bundle for this fleet yet. Fetch it from its repository first, then publish.",
        ),
    },
    Problem {
        code: error_code::CATALOG_DELETE_PUBLISHED,
        status: 409,
        title: "Cannot delete a published fleet",
        hint: "This fleet is published and installable. Unpublish it first, then delete it.",
        user_message: Some("This fleet is published. Unpublish it first, then delete it."),
    },
    Problem {
        code: error_code::CATALOG_ID_COLLISION,
        status: 409,
        title: "Catalog id already taken by another repository",
        hint: "This catalog id already belongs to a different source repository. Rename the bundle, or retry with replace to overwrite deliberately.",
        user_message: Some(
            "A different repository already owns this fleet's name. Rename the bundle, or confirm you want to replace it.",
        ),
    },
    Problem {
        code: error_code::CATALOG_ROW_STALE,
        status: 412,
        title: "Catalog entry changed since you loaded it",
        hint: "Another operator saved first: `If-Match` names an old version. Refetch the row, re-apply your edit, and retry with the new `etag`.",
        user_message: Some(
            "Someone else edited this catalog entry since you opened it. Refresh to see their change, then re-apply your edit.",
        ),
    },
];
