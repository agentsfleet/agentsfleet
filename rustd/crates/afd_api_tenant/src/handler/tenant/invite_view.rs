//! An invite as the owner sees it on the wire, and the link that accepts it.
//!
//! Shared by the invite routes and the send-again route, which both answer
//! with a link or need one for the email.

use std::borrow::Cow;

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_tenant::team::Invitation;
use afd_wire::team::InviteSummary;
use url::Url;

use crate::handler::Refusal;

/// The refusal a dashboard base that is not a URL earns: a deployment fault.
const DETAIL_LINK: &str = "Invite link could not be built";

/// The dashboard path an invite's accept page lives under.
const INVITES_PATH: &str = "invites";

/// One invite, with the dashboard page that accepts it.
pub(super) fn summary<'a>(
    dashboard: &str,
    invite: &'a Invitation,
) -> Result<InviteSummary<'a>, Refusal> {
    let link = link_or_refuse(dashboard, &invite.id)?;
    Ok(InviteSummary {
        id: Cow::Borrowed(invite.id.as_str()),
        email: Cow::Borrowed(&invite.email),
        role: Cow::Borrowed(invite.role.wire()),
        expires_at: invite.expires_at_ms,
        created_at: invite.created_at_ms,
        link: Cow::Owned(link),
        email_status: Cow::Borrowed(invite.email_status.wire()),
        email_sent_at: invite.email_sent_at_ms,
    })
}

/// The accept link, or the refusal a dashboard base that is not a URL earns.
pub(super) fn link_or_refuse(dashboard: &str, invite: &Uuid7) -> Result<String, Refusal> {
    invite_link(dashboard, invite)
        .ok_or_else(|| Refusal::coded(error_code::INTERNAL_OPERATION_FAILED, DETAIL_LINK))
}

/// `{dashboard}/invites/{invite_id}`, built by path segment.
///
/// Through `path_segments_mut`, as the connector's own dashboard URLs are,
/// so a base carrying a trailing slash or a sub-path still yields one
/// well-formed URL. `None` for a base that is not a URL.
fn invite_link(dashboard: &str, invite: &Uuid7) -> Option<String> {
    let mut url = Url::parse(dashboard).ok()?;
    url.path_segments_mut()
        .ok()?
        .pop_if_empty()
        .extend([INVITES_PATH, invite.as_str()]);
    Some(url.into())
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "test module: an unmet precondition should fail the test loudly"
)]
mod tests {
    use afd_core::id::Uuid7;

    use super::invite_link;

    const INVITE: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1011";

    #[test]
    fn an_invite_link_is_one_well_formed_url_whatever_the_base_ends_with() {
        let invite = Uuid7::parse(INVITE).expect("the fixture identifier is UUIDv7");
        for base in [
            "https://app.test",
            "https://app.test/",
            "https://app.test/dash/",
        ] {
            let link = invite_link(base, &invite).expect("a URL base yields a link");
            assert!(
                link.ends_with(&format!("/invites/{INVITE}")),
                "{base} gave {link}"
            );
            assert!(!link.contains("//invites"), "{base} gave {link}");
        }
        assert_eq!(invite_link("not a url", &invite), None);
    }
}
