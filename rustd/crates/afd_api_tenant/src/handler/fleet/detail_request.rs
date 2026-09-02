//! Turning a fleet PATCH body into the [`Patch`] the lifecycle plane is handed.
//!
//! Split from [`super::detail`] along the line the file cap and the tests both
//! want: everything here is total, synchronous and datastore-free, so the
//! refusal surface in front of the write is proven without driving HTTP. Every
//! ambiguity is resolved here, once, into a type that cannot hold it.

use afd_fleet_lifecycle::{ConfigSource, Patch, Requested};
use afd_wire::fleet::PatchFleetRequest;
use axum::body::Bytes;

use super::detail::{
    DETAIL_CONFIG_AMBIGUOUS, DETAIL_CONFIG_REQUIRED, DETAIL_MALFORMED_JSON, DETAIL_SOURCE_BOUNDS,
    DETAIL_STATUS_INVALID, DETAIL_TRIGGER_BOUNDS,
};
use crate::handler::Refusal;

/// The most bytes an authored document may carry.
///
/// The sentences above say 64KiB and this says two hundred. The mismatch is in
/// the Zig too, and it is the NUMBER that is load-bearing — ported as-is,
/// because a client sitting between the two would change class if either moved.
const MAX_MARKDOWN_LEN: usize = 200 * 1024;

/// The PATCH the body asks for, or the refusal it earns.
///
/// Every ambiguity is resolved HERE, once, into a type that cannot hold it: the
/// two configuration sources become one [`ConfigSource`], and the status becomes
/// a [`Requested`] that cannot spell `paused`.
pub(super) fn read_patch(body: &Bytes, if_match: Option<String>) -> Result<Patch, Refusal> {
    if body.is_empty() {
        return Ok(Patch {
            if_match,
            ..Patch::default()
        });
    }
    let sent = afd_core::json::object_from_slice::<PatchFleetRequest<'_>>(body)
        .map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED_JSON))?;

    let config = match (
        sent.config_json.as_deref(),
        sent.trigger_markdown.as_deref(),
    ) {
        // Both drive `core.fleets.config_json`, so there is no answer to which
        // one wins — refused at the door rather than resolved by precedence.
        (Some(_json), Some(_document)) => return Err(Refusal::malformed(DETAIL_CONFIG_AMBIGUOUS)),
        (Some(""), None) => return Err(Refusal::malformed(DETAIL_CONFIG_REQUIRED)),
        (Some(json), None) => Some(ConfigSource::Json(json.to_owned())),
        (None, Some(document)) => Some(ConfigSource::Trigger(
            bounded(document, DETAIL_TRIGGER_BOUNDS)?.to_owned(),
        )),
        (None, None) => None,
    };
    let source_markdown = sent
        .source_markdown
        .as_deref()
        .map(|document| bounded(document, DETAIL_SOURCE_BOUNDS).map(str::to_owned))
        .transpose()?;

    Ok(Patch {
        config,
        status: sent.status.as_deref().map(requested).transpose()?,
        source_markdown,
        if_match,
    })
}

/// The document, if it is one this daemon will store.
fn bounded<'a>(document: &'a str, detail: &'static str) -> Result<&'a str, Refusal> {
    if document.is_empty() || document.len() > MAX_MARKDOWN_LEN {
        return Err(Refusal::malformed(detail));
    }
    Ok(document)
}

/// The transition a spelling asks for, or the refusal an unknown one earns.
///
/// `paused` is refused here rather than accepted and ignored: it belongs to the
/// platform's anomaly gate, and admitting it would let a caller forge a
/// system-halt provenance on their own fleet.
fn requested(spelling: &str) -> Result<Requested, Refusal> {
    match spelling {
        "active" => Ok(Requested::Active),
        "stopped" => Ok(Requested::Stopped),
        "killed" => Ok(Requested::Killed),
        _reserved_or_unknown => Err(Refusal::malformed(DETAIL_STATUS_INVALID)),
    }
}
