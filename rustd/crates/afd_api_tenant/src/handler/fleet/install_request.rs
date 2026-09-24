//! Turning an install body into the [`Install`] the lifecycle plane is handed.
//!
//! Split from [`super`] along the line [`super::detail`] and its
//! `detail_request` already draw: everything here is total, synchronous and
//! datastore-free, so the refusal surface in front of an install is proven
//! without driving HTTP. Each field is parsed once into a type that cannot hold
//! a bad value — a library tier, a [`FleetName`], a [`ChannelId`].

use afd_connector::Provider;
use afd_core::id::Uuid7;
use afd_fleet_lifecycle::{Install, LibrarySource};
use afd_fleet_runtime::FleetName;
use afd_fleet_runtime::config::{ChannelId, Mention};
use afd_wire::fleet::InstallFleetRequest;

use crate::handler::Refusal;

/// The refusal an install body this daemon cannot read earns.
pub const DETAIL_MALFORMED_JSON: &str = "Request body is not valid JSON";

/// The refusal an install naming no library entry earns.
pub const DETAIL_LIBRARY_REQUIRED: &str =
    "install requires platform_library_id or tenant_library_id";

/// The refusal an install naming both tiers earns.
pub const DETAIL_LIBRARY_AMBIGUOUS: &str =
    "install accepts exactly one of platform_library_id or tenant_library_id";

/// The refusal a name override this daemon will not store earns.
pub const DETAIL_NAME_INVALID: &str = "name is required (max 64 chars, slug-safe)";

/// The refusal a tenant library id that is not an identifier earns.
pub const DETAIL_TENANT_LIBRARY_ID: &str = "tenant_library_id must be a valid UUIDv7";

/// The refusal a Slack channel that is not a channel identifier earns.
pub const DETAIL_SLACK_CHANNEL_ID: &str =
    "slack_channel_id must be a channel identifier: C or G, then upper-case letters and digits";

/// The body an empty POST reads as — `req.body() orelse "{}"`, ported.
const EMPTY_OBJECT: &[u8] = b"{}";

/// The install body, or the refusal a body this daemon cannot read earns.
///
/// # Errors
/// [`DETAIL_MALFORMED_JSON`] for a body that is not the request's JSON.
pub(super) fn read_body(body: &[u8]) -> Result<InstallFleetRequest<'_>, Refusal> {
    let body = if body.is_empty() { EMPTY_OBJECT } else { body };
    afd_http::handler::read_body::<InstallFleetRequest<'_>>(body)
        .map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED_JSON))
}

/// What the install asks for, every field parsed.
///
/// # Errors
/// The refusal the first unusable field earns: the library tier, the name,
/// then the Slack channel.
pub(super) fn read_install<'a>(
    request: &'a InstallFleetRequest<'a>,
) -> Result<Install<'a>, Refusal> {
    let source = library_source(request)?;
    let name = request
        .name
        .as_deref()
        .map(FleetName::parse)
        .transpose()
        .map_err(|_unusable| Refusal::malformed(DETAIL_NAME_INVALID))?;
    let mention = request
        .slack_channel_id
        .as_deref()
        .map(slack_mention)
        .transpose()?;
    Ok(Install {
        source,
        name,
        mention,
    })
}

/// Which library tier this install draws from, or the refusal it earns.
///
/// The neither-set and both-set cases are two different sentences, which is why
/// the wire struct carries two optional fields rather than an untagged enum: a
/// parse failure could not tell a caller which of the two they did.
fn library_source<'a>(request: &'a InstallFleetRequest<'a>) -> Result<LibrarySource<'a>, Refusal> {
    match (
        request.platform_library_id.as_deref(),
        request.tenant_library_id.as_deref(),
    ) {
        (Some(_platform), Some(_tenant)) => Err(Refusal::malformed(DETAIL_LIBRARY_AMBIGUOUS)),
        (Some(platform), None) => Ok(LibrarySource::Platform(platform)),
        (None, Some(tenant)) => Uuid7::parse(tenant)
            .map(LibrarySource::Tenant)
            .map_err(|_not_an_identifier| Refusal::malformed(DETAIL_TENANT_LIBRARY_ID)),
        (None, None) => Err(Refusal::malformed(DETAIL_LIBRARY_REQUIRED)),
    }
}

/// The `mention` trigger a Slack channel identifier attaches.
///
/// The identifier is checked by [`ChannelId`]'s own parser, the one a stored
/// document's trigger goes through, so the install and the document cannot
/// disagree about what a channel is.
fn slack_mention(channel: &str) -> Result<Mention, Refusal> {
    let channel = channel
        .parse::<ChannelId>()
        .map_err(|_not_a_channel| Refusal::malformed(DETAIL_SLACK_CHANNEL_ID))?;
    Ok(Mention {
        source: Provider::Slack.id().into(),
        channel,
    })
}
