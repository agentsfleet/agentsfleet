//! The refusal body, described for the document.
//!
//! # Why one schema and not one per status
//!
//! Every refusal this daemon writes goes through [`crate::envelope`], and that
//! writer has one shape: the five fields every refusal carries, and the
//! extensions a status adds to them. A 409 is the base envelope plus
//! `current_state`; a 412 is the base envelope plus `etag`. Publishing a
//! schema per status would say the same five fields eleven times and drift on
//! the twelfth, so the document names one body and marks the extensions
//! optional, which is exactly the switch-on-presence RFC 7807 invites a client
//! to make.
//!
//! # This describes the wire, not the writer
//!
//! Nothing here is constructed at runtime. [`crate::envelope::ProblemResponse`]
//! builds the body it sends as a map, in the field order the Zig envelope
//! wrote; this type is what the document says about that body, and the
//! substrate suite holds the two to the same field set.

use utoipa::ToSchema;

/// The body every refusal carries, under `application/problem+json`.
#[derive(Debug, ToSchema)]
pub struct ProblemBody {
    /// Where the error code is documented.
    pub docs_uri: String,
    /// A short name for the refusal, safe to show a person.
    pub title: String,
    /// One sentence on what was refused and why.
    pub detail: String,
    /// The registry code, stable across releases.
    pub error_code: String,
    /// The request this refusal answers, for support.
    pub request_id: String,
    /// On a 409 only: the state that forbade the transition.
    pub current_state: Option<String>,
    /// On a 412 only: the resource's current entity tag, to refetch and retry.
    pub etag: Option<String>,
    /// On a 424 only: the credentials this workspace has yet to store.
    pub missing_secrets: Option<Vec<String>>,
    /// A curated sentence for end users, where the code has one.
    pub user_message: Option<String>,
}
