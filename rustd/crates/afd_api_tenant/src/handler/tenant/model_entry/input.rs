//! What the registry accepts from a request, before any store is reached.
//!
//! Split from the handler half at the file cap, along the seam the page
//! already had: everything here refuses from the REQUEST alone — a token that
//! does not belong to this walk, an id that is not a `UUIDv7`, a model name
//! over the bound. None of it touches a pool, which is what lets the input
//! bounds be proven at router tier with no Postgres.
//!
//! The page SIZE is the one bound that left: the gallery next door holds
//! callers to the identical rule and told them so in the identical sentence,
//! so it is declared once in [`crate::handler::paging`] and read from here.

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_core::paging::QUERY_STARTING_AFTER;
use afd_core::paging::struct_cursor;
use afd_credential::provider::Boundary;
use afd_validate::Sentences;

use crate::handler::tenant::models::DETAIL_CURSOR_MALFORMED;
use crate::handler::{Refusal, parameter};

use super::{
    Cursor, DETAIL_CURSOR_MISMATCH, DETAIL_ENTRY_ID, DETAIL_MODEL_ID_REQUIRED,
    DETAIL_MODEL_ID_TOO_LONG, DETAIL_SECRET_REF_REQUIRED,
};

/// The boundary this request resumes from, or nothing for the first page.
///
/// The identity check is here and not in the store: only this function knows
/// which tenant authenticated and which limit was asked for, which is the whole
/// reason the seam takes a [`Boundary`] rather than a token.
pub(super) fn resume_from(
    raw: &str,
    tenant: &Uuid7,
    limit: u32,
) -> Result<Option<Boundary>, Refusal> {
    let Some(token) = parameter(raw, QUERY_STARTING_AFTER).filter(|token| !token.is_empty()) else {
        return Ok(None);
    };
    let cursor: Cursor = struct_cursor::parse(token).map_err(|_foreign| {
        Refusal::coded(
            error_code::LIBRARY_CURSOR_MALFORMED,
            DETAIL_CURSOR_MALFORMED,
        )
    })?;
    if cursor.tenant_uuid != tenant.as_str() || cursor.limit != limit {
        return Err(Refusal::coded(
            error_code::LIBRARY_CURSOR_MISMATCH,
            DETAIL_CURSOR_MISMATCH,
        ));
    }
    // The id is the only field taken from the token besides the instant, and it
    // is re-parsed rather than trusted: a `::uuid` cast is not the place to
    // discover that a client sent something else.
    let id = Uuid7::parse(&cursor.id).map_err(|_not_an_identifier| {
        Refusal::coded(
            error_code::LIBRARY_CURSOR_MALFORMED,
            DETAIL_CURSOR_MALFORMED,
        )
    })?;
    Ok(Some(Boundary {
        created_at_ms: cursor.created_at,
        id,
    }))
}

/// The entry a path segment names.
pub(super) fn parse_entry_id(raw: &str) -> Result<Uuid7, Refusal> {
    Uuid7::parse(raw).map_err(|_not_an_identifier| Refusal::malformed(DETAIL_ENTRY_ID))
}

/// The refusal for the bound a request body broke.
///
/// The BOUNDS live on the request types in [`afd_api_wire::tenant_model_entry`];
/// the wording stays here because the dashboard renders it. A blank and an
/// oversized `model_id` break one bound at one path yet are different repairs,
/// so the value picks the table and the table picks the sentence. A `model_id`
/// break wins over a `secret_ref` one: it is listed first.
pub(super) fn bound_refusal(report: &garde::Report, model_id: &str) -> Refusal {
    let table = if model_id.is_empty() {
        &BOUNDS_BLANK_MODEL_ID
    } else {
        &BOUNDS
    };
    Refusal::malformed(table.pick(report))
}

/// The sentences for a body whose `model_id` is present.
const BOUNDS: Sentences = Sentences::new(
    &[
        (FIELD_MODEL_ID, DETAIL_MODEL_ID_TOO_LONG),
        (FIELD_SECRET_REF, DETAIL_SECRET_REF_REQUIRED),
    ],
    DETAIL_MODEL_ID_REQUIRED,
);

/// The sentences for a body whose `model_id` is blank.
const BOUNDS_BLANK_MODEL_ID: Sentences = Sentences::new(
    &[
        (FIELD_MODEL_ID, DETAIL_MODEL_ID_REQUIRED),
        (FIELD_SECRET_REF, DETAIL_SECRET_REF_REQUIRED),
    ],
    DETAIL_MODEL_ID_REQUIRED,
);

/// The path `garde` reports a model-identity break under.
const FIELD_MODEL_ID: &str = "model_id";
/// The path `garde` reports a credential-reference break under.
const FIELD_SECRET_REF: &str = "secret_ref";
