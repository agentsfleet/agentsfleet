//! The workspace fleets surface's payloads: list, install, detail, PATCH.
//!
//! # The trigger projection rides as raw JSON
//!
//! Every response carrying a fleet also carries `triggers`, projected from
//! `config_json->'x-agentsfleet'->'triggers'`. The store hands it up as the
//! stored TEXT and it is spliced here through [`RawValue`], never parsed into a
//! tree and re-serialized. That is not only cheaper — a re-serialize would
//! re-order keys and re-format numbers, and this field is the one a dashboard
//! renders per-trigger cards from.
//!
//! Text that will not parse renders as `null`, which is `parseFromSlice(…)
//! catch null` kept exactly: a legacy row with a malformed projection shows no
//! triggers rather than failing the whole page.
//!
//! # Nulls stay on the wire
//!
//! No `skip_serializing_if`, for this crate's usual reason: the Zig emitter
//! writes `null` for an absent optional on a SUCCESS body, and dropping the key
//! would be a shape change a client can see.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

/// The stored trigger projection, spliced without being parsed.
///
/// `Box<RawValue>` and not `Value`: `RawValue::from_string`
/// validates that the text IS JSON and then emits those bytes verbatim, so a
/// caller gets the field the way the configuration was authored.
///
/// The types carrying one derive no `PartialEq` as a result — `serde_json`
/// gives `RawValue` none on purpose, because two spellings of one document are
/// equal as JSON and different as text, and it will not guess which a caller
/// meant. A suite asserts on the serialized body, which is the thing that
/// actually goes over the wire.
pub type Triggers = Option<Box<RawValue>>;

/// `POST /v1/workspaces/{workspace_id}/fleets` — install one.
///
/// Unknown fields are IGNORED, matching `create.zig`'s
/// `.ignore_unknown_fields = true`, and the parity is kept by the ABSENCE of a
/// serde attribute.
///
/// Exactly one library id is required. Both fields are optional HERE because
/// the refusal for neither and the refusal for both are different sentences,
/// and a `#[serde(untagged)]` enum would collapse them into one parse failure
/// the caller could not act on.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct InstallFleetRequest<'a> {
    /// A published platform entry, by slug.
    #[serde(borrow, default)]
    pub platform_library_id: Option<Cow<'a, str>>,
    /// This workspace's own entry, by identifier.
    #[serde(borrow, default)]
    pub tenant_library_id: Option<Cow<'a, str>>,
    /// The operator's name for this instance, when they want one.
    ///
    /// The same bundle backs many fleets in a workspace, each with its own name
    /// and its own webhooks. Absent means the bundle's declared name.
    #[serde(borrow, default)]
    pub name: Option<Cow<'a, str>>,
}

/// What an install answers with.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstalledFleetResponse<'a> {
    /// The new fleet's identifier.
    pub fleet_id: Cow<'a, str>,
    /// The name it was stored under — the drawn one, when a default collided.
    pub name: Cow<'a, str>,
    /// Where it stands. `active`: the stream exists, so it is already leasable.
    pub status: Cow<'a, str>,
    /// One entry per webhook trigger the bundle declared, keyed by source.
    ///
    /// An empty object where the fleet declares none, never `null`: a client
    /// iterating the map should not have to branch on its absence first.
    pub webhook_urls: Vec<WebhookUrl<'a>>,
}

/// Where one declared webhook trigger is delivered.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WebhookUrl<'a> {
    /// The provider that sends it.
    pub source: Cow<'a, str>,
    /// The address it posts to, on this deployment.
    pub url: Cow<'a, str>,
}

/// One fleet as a list page shows it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize)]
pub struct FleetSummary<'a> {
    /// The fleet's identifier.
    pub id: Cow<'a, str>,
    /// Its name in this workspace.
    pub name: Cow<'a, str>,
    /// Where it stands in its life.
    pub status: Cow<'a, str>,
    /// When it was installed.
    pub created_at: i64,
    /// When it last changed.
    pub updated_at: i64,
    /// What may wake it, from the stored configuration.
    ///
    /// [`Triggers`] is `Option<Box<RawValue>>`: a stored document spliced
    /// through unparsed. `value_type` names the serialized shape, which is the
    /// only thing a schema can say about it.
    #[cfg_attr(feature = "openapi", schema(value_type = Option<Object>))]
    pub triggers: Triggers,
    /// Lifetime event count. Server truth, never client arithmetic.
    pub events_processed: i64,
    /// Lifetime spend, in nanos.
    pub budget_used_nanos: i64,
}

/// `GET /v1/workspaces/{workspace_id}/fleets` — one page, newest first.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize)]
pub struct FleetsResponse<'a> {
    /// The fleets on this page.
    pub items: Vec<FleetSummary<'a>>,
    /// How many are on this page — `list.zig` answers the page length, not the
    /// workspace's whole count, and the name is the one that shipped.
    pub total: usize,
    /// Where the next page resumes, or `null` on the last one.
    pub next_cursor: Option<Cow<'a, str>>,
}

/// `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}` — one fleet, whole.
///
/// The list row's fields plus the editable surface. Flattened rather than
/// nested under a `summary` key, because that is the shape the source editor
/// already reads.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize)]
pub struct FleetDetailResponse<'a> {
    /// The fleet's identifier.
    pub id: Cow<'a, str>,
    /// Its name in this workspace.
    pub name: Cow<'a, str>,
    /// Where it stands in its life.
    pub status: Cow<'a, str>,
    /// The authored `SKILL.md`, verbatim.
    pub source_markdown: Cow<'a, str>,
    /// The authored `TRIGGER.md`, or `null` where the bundle carried none.
    pub trigger_markdown: Option<Cow<'a, str>>,
    /// The bundle a runner materialises support files from.
    pub bundle_content_hash: Option<Cow<'a, str>>,
    /// What may wake it, from the stored configuration.
    ///
    /// [`Triggers`] is `Option<Box<RawValue>>`: a stored document spliced
    /// through unparsed. `value_type` names the serialized shape, which is the
    /// only thing a schema can say about it.
    #[cfg_attr(feature = "openapi", schema(value_type = Option<Object>))]
    pub triggers: Triggers,
    /// Lifetime event count.
    pub events_processed: i64,
    /// Lifetime spend, in nanos.
    pub budget_used_nanos: i64,
    /// When it was installed.
    pub created_at: i64,
    /// When it last changed.
    pub updated_at: i64,
}

/// `PATCH /v1/workspaces/{workspace_id}/fleets/{fleet_id}` — a partial update.
///
/// Every field is optional and presence-based; an empty body is a no-op that
/// touches no row. `config_json` and `trigger_markdown` both drive the stored
/// configuration and are mutually exclusive — sent together they are refused at
/// the door, because there is no answer to which one wins.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct PatchFleetRequest<'a> {
    /// A configuration document, replacing the stored one directly.
    #[serde(borrow, default)]
    pub config_json: Option<Cow<'a, str>>,
    /// The transition asked for: `active`, `stopped` or `killed`.
    ///
    /// `paused` is refused — it belongs to the platform's anomaly gate, and
    /// accepting it here would let a caller forge a system-halt provenance.
    #[serde(borrow, default)]
    pub status: Option<Cow<'a, str>>,
    /// An authored `TRIGGER.md`, reparsed into the configuration and the name.
    #[serde(borrow, default)]
    pub trigger_markdown: Option<Cow<'a, str>>,
    /// A replacement `SKILL.md`, cross-checked against the name.
    #[serde(borrow, default)]
    pub source_markdown: Option<Cow<'a, str>>,
}

/// What a PATCH answers with — one of three shapes, as `patch.zig` writes them.
///
/// An untagged enum rather than one struct with optional keys, and that is the
/// rule this crate already keeps rather than an exception to it: no
/// `skip_serializing_if` appears here, because each variant emits exactly the
/// keys its case carries. A config-only edit never wrote a `status` key, and a
/// no-op never wrote an `etag` — with one struct those would have to be skipped
/// conditionally, which is the thing that silently drops a field somebody adds
/// later.
///
/// It also makes two impossible responses impossible: an `etag` without a
/// revision, and a revision on the no-op.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum PatchedFleetResponse<'a> {
    /// An empty body: nothing was written, and no row was even read.
    Unchanged {
        /// The fleet the request named.
        fleet_id: Cow<'a, str>,
        /// Always `null` — the literal `@as(?i64, null)` the Zig answers, kept
        /// because a client distinguishes "no write" from a revision by it.
        config_revision: Option<i64>,
    },
    /// A configuration or source edit, with no transition asked for.
    Changed {
        /// The fleet that changed.
        fleet_id: Cow<'a, str>,
        /// The new revision — `updated_at`, echoed back by a conditional caller.
        config_revision: i64,
        /// The tag over the post-update source, for the editor's next save.
        etag: Cow<'a, str>,
    },
    /// A transition, with or without an edit beside it.
    Transitioned {
        /// The fleet that changed.
        fleet_id: Cow<'a, str>,
        /// The status it now holds.
        status: Cow<'a, str>,
        /// The new revision.
        config_revision: i64,
        /// The tag over the post-update source.
        etag: Cow<'a, str>,
    },
}
