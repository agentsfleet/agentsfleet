//! The gallery page and the onboarded entry, rendered onto the wire.
//!
//! Split from the handlers by what changes together: a field added to a card
//! lands here and in `afd_wire::workspace_library`, and touches nothing about
//! who may read the page or how a cursor resumes it.

use std::borrow::Cow;

use afd_core::paging::struct_cursor;
use afd_library::{GalleryPage, Onboarded, SummaryEntry, Tier};
use afd_wire::admin::{AdminLibraryCreated, AdminLibraryRequirements};
use afd_wire::workspace_library::{GalleryCard, GalleryResponse};

use super::Cursor;

/// The page, rendered.
pub(super) fn rendered<'p>(
    page: &'p GalleryPage,
    workspace: &str,
    limit: u32,
) -> GalleryResponse<'p> {
    GalleryResponse {
        items: page.items.iter().map(card).collect(),
        // Always null: counting a keyset page costs the scan this pagination
        // exists to avoid, and the key stays present rather than vanishing.
        total: None,
        next_cursor: page.next.as_ref().map(|position| {
            struct_cursor::render(&Cursor {
                v: struct_cursor::VERSION,
                created_at: position.created_at_ms,
                tier_rank: position.tier.rank(),
                id: position.id.clone(),
                workspace_uuid: workspace.to_owned(),
                limit,
            })
        }),
    }
}

/// One card, rendered.
///
/// `visibility` is the TIER's label — see [`afd_wire::workspace_library`] on why
/// that field name carries a different fact here than on the admin surface.
fn card(entry: &SummaryEntry) -> GalleryCard<'_> {
    GalleryCard {
        id: Cow::Borrowed(&entry.id),
        name: Cow::Borrowed(&entry.name),
        description: Cow::Borrowed(&entry.description),
        visibility: Cow::Borrowed(entry.tier.label()),
        source_ref: Cow::Borrowed(&entry.source_ref),
        created_at: entry.created_at_ms,
        requirements: requirements(&entry.requirements),
        required_credentials_reasons: entry.required_credentials_reasons.clone(),
    }
}

/// What a bundle declares it needs, rendered.
///
/// Every name is BORROWED onto the wire. A page is up to a hundred cards and
/// each carries three lists, so copying them would be the one allocation on
/// this path that scales with the page.
fn requirements(declared: &afd_library::LibraryRequirements) -> AdminLibraryRequirements<'_> {
    AdminLibraryRequirements {
        credentials: borrowed(declared.credentials()),
        tools: borrowed(declared.tools()),
        network_hosts: borrowed(declared.network_hosts()),
        trigger_present: declared.trigger_present(),
    }
}

/// One declared list, borrowed rather than copied.
fn borrowed(names: &[String]) -> Vec<Cow<'_, str>> {
    names
        .iter()
        .map(|name| Cow::Borrowed(name.as_str()))
        .collect()
}

/// The onboarded entry, rendered.
///
/// The same shape the operator's catalogue answers with, because both verbs say
/// the same thing — which entry now stands — and the tier is what differs.
pub(super) fn created(onboarded: Onboarded) -> AdminLibraryCreated<'static> {
    let bundle = onboarded.bundle;
    let declared = bundle.requirements;
    AdminLibraryCreated {
        id: Cow::Owned(onboarded.id),
        name: Cow::Owned(bundle.name),
        visibility: Cow::Borrowed(Tier::Tenant.label()),
        content_hash: Cow::Owned(bundle.content_hash),
        requirements: AdminLibraryRequirements {
            credentials: declared.credentials.into_iter().map(Cow::Owned).collect(),
            tools: declared.tools.into_iter().map(Cow::Owned).collect(),
            network_hosts: declared.network_hosts.into_iter().map(Cow::Owned).collect(),
            trigger_present: declared.trigger_present,
        },
    }
}
