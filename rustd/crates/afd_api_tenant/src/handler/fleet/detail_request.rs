//! Turning a fleet PATCH body into the [`Patch`] the lifecycle plane is handed.
//!
//! Split from [`super::detail`] along the line the file cap and the tests both
//! want: everything here is total, synchronous and datastore-free, so the
//! refusal surface in front of the write is proven without driving HTTP. Every
//! ambiguity is resolved here, once, into a type that cannot hold it.

use afd_fleet_lifecycle::{ConfigSource, Patch, Requested};
use afd_wire::fleet::PatchFleetRequest;
use axum::body::Bytes;

use garde::Validate as _;

use crate::handler::Refusal;

/// The refusal a PATCH body this daemon cannot read earns.
pub const DETAIL_MALFORMED_JSON: &str = "Request body is not valid JSON";

/// The refusal a PATCH naming both configuration sources earns.
pub const DETAIL_CONFIG_AMBIGUOUS: &str = "config_json and trigger_markdown are mutually exclusive";

/// The refusal an empty `config_json` earns.
pub const DETAIL_CONFIG_REQUIRED: &str = "config_json is required";

/// The refusal a status outside the operator-targetable set earns.
pub const DETAIL_STATUS_INVALID: &str = "status must be one of \"active\", \"stopped\", \"killed\"";

/// The refusal a document outside its length bounds earns.
pub const DETAIL_TRIGGER_BOUNDS: &str = "trigger_markdown must be 1..64KiB";

/// The refusal a source document outside its length bounds earns.
pub const DETAIL_SOURCE_BOUNDS: &str = "source_markdown must be 1..64KiB";

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

    // Which SOURCES were named is settled before how long they are. Both
    // questions refuse the same request, and this one is about the PAIR — no
    // per-field bound can express it, and a caller who sent both needs that
    // answer rather than a length.
    let sources = match (
        sent.config_json.as_deref(),
        sent.trigger_markdown.as_deref(),
    ) {
        // Both drive `core.fleets.config_json`, so there is no answer to which
        // one wins — refused at the door rather than resolved by precedence.
        (Some(_json), Some(_document)) => return Err(Refusal::malformed(DETAIL_CONFIG_AMBIGUOUS)),
        (Some(""), None) => return Err(Refusal::malformed(DETAIL_CONFIG_REQUIRED)),
        named => named,
    };
    sent.validate().map_err(|report| detail_for(&report))?;

    let config = match sources {
        (Some(json), None) => Some(ConfigSource::Json(json.to_owned())),
        (None, Some(document)) => Some(ConfigSource::Trigger(document.to_owned())),
        // Both-named and an empty `config_json` returned above; what is left is
        // the patch that changes neither.
        _neither => None,
    };
    let source_markdown = sent.source_markdown.as_deref().map(str::to_owned);

    Ok(Patch {
        config,
        status: sent.status.as_deref().map(requested).transpose()?,
        source_markdown,
        if_match,
    })
}

/// The sentence a caller is told, for the bound their body broke.
///
/// The BOUND lives on [`PatchFleetRequest`]; what stays here is which of the
/// two sentences a break earns, because `trigger_markdown` and
/// `source_markdown` share one cap and answer different copy. `garde` reports a
/// PATH and a message — the path picks the wording, and the message is
/// discarded, because these two sentences are a public contract and garde's are
/// not.
fn detail_for(report: &garde::Report) -> Refusal {
    let detail = report
        .iter()
        .next()
        .map_or(DETAIL_TRIGGER_BOUNDS, |(path, _message)| {
            if path.to_string() == FIELD_SOURCE_MARKDOWN {
                DETAIL_SOURCE_BOUNDS
            } else {
                DETAIL_TRIGGER_BOUNDS
            }
        });
    Refusal::malformed(detail)
}

/// The path `garde` reports a `source_markdown` break under.
const FIELD_SOURCE_MARKDOWN: &str = "source_markdown";

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

#[cfg(test)]
mod tests {
    use axum::body::Bytes;
    use axum::response::IntoResponse as _;
    use http::StatusCode;

    // The cap is read from the type that DECLARES it, never copied: a local
    // number would let the bound move on `PatchFleetRequest` while these cases
    // asserted the old one and still passed.
    use afd_wire::fleet::FLEET_MARKDOWN_MAX_BYTES as MAX_MARKDOWN_LEN;

    use super::{
        DETAIL_CONFIG_REQUIRED, DETAIL_SOURCE_BOUNDS, DETAIL_STATUS_INVALID, DETAIL_TRIGGER_BOUNDS,
        read_patch,
    };

    /// The refusal a body earns, as its status and the sentence a caller reads.
    async fn refused(body: &str) -> Option<(StatusCode, String)> {
        let response = read_patch(&Bytes::copy_from_slice(body.as_bytes()), None)
            .err()?
            .into_response();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .ok()?;
        let document: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        let detail = document.get("detail")?.as_str()?.to_owned();
        Some((status, detail))
    }

    /// An empty `config_json` is a request to write nothing, refused as such.
    #[tokio::test]
    async fn test_an_empty_config_json_is_refused_as_required() {
        assert_eq!(
            refused(r#"{"config_json":""}"#).await,
            Some((StatusCode::BAD_REQUEST, DETAIL_CONFIG_REQUIRED.to_owned()))
        );
    }

    /// A document is refused when empty or past the cap, and kept at the cap.
    #[tokio::test]
    async fn test_a_document_is_refused_when_empty_or_past_the_cap_and_kept_at_it() {
        assert_eq!(
            refused(r#"{"trigger_markdown":""}"#).await,
            Some((StatusCode::BAD_REQUEST, DETAIL_TRIGGER_BOUNDS.to_owned()))
        );
        assert_eq!(
            refused(r#"{"source_markdown":""}"#).await,
            Some((StatusCode::BAD_REQUEST, DETAIL_SOURCE_BOUNDS.to_owned()))
        );

        let at_cap = format!(
            r#"{{"source_markdown":"{}"}}"#,
            "x".repeat(MAX_MARKDOWN_LEN)
        );
        assert!(
            read_patch(&Bytes::from(at_cap), None).is_ok(),
            "the cap is inclusive: a document exactly at it is stored"
        );
        let past_cap = format!(
            r#"{{"trigger_markdown":"{}"}}"#,
            "x".repeat(MAX_MARKDOWN_LEN + 1)
        );
        assert_eq!(
            refused(&past_cap).await,
            Some((StatusCode::BAD_REQUEST, DETAIL_TRIGGER_BOUNDS.to_owned()))
        );
    }

    /// `paused` belongs to the platform's anomaly gate, never to a caller.
    #[tokio::test]
    async fn test_the_platform_owned_status_is_refused() {
        assert_eq!(
            refused(r#"{"status":"paused"}"#).await,
            Some((StatusCode::BAD_REQUEST, DETAIL_STATUS_INVALID.to_owned()))
        );
    }
}
