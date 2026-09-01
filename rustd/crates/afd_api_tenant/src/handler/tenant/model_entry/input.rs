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

use crate::handler::tenant::models::DETAIL_CURSOR_MALFORMED;
use crate::handler::{Refusal, parameter};

use super::{
    Cursor, DETAIL_CURSOR_MISMATCH, DETAIL_ENTRY_ID, DETAIL_MODEL_ID_REQUIRED,
    DETAIL_MODEL_ID_TOO_LONG, MODEL_ID_MAX,
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

/// A model name within its bound, or the refusal it earns.
///
/// Blank and oversized are different sentences because the repairs differ, and
/// the bound is checked here rather than at the store: a name past it is a
/// malformed REQUEST, and the column would take it.
pub(super) fn bounded_model(model_id: &str) -> Result<&str, Refusal> {
    if model_id.is_empty() {
        return Err(Refusal::malformed(DETAIL_MODEL_ID_REQUIRED));
    }
    if model_id.len() > MODEL_ID_MAX {
        return Err(Refusal::malformed(DETAIL_MODEL_ID_TOO_LONG));
    }
    Ok(model_id)
}
