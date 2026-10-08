//! An invite as the owner sees it on the wire, and the link that accepts it.
//!
//! Shared by the invite routes and the send-again route, which both answer
//! with a link or need one for the email.

use std::borrow::Cow;

use afd_api_wire::team::InviteSummary;
use afd_connector::Dashboard;
use afd_core::id::Uuid7;
use afd_tenant::team::Invitation;

/// The dashboard path an invite's accept page lives under.
const INVITES_PATH: &str = "invites";

/// One invite, with `link`, the dashboard page that accepts it.
///
/// The link comes in built rather than built here: the create already built it
/// for the email, and a list builds one per row off a base parsed at boot.
pub(super) fn summary(invite: &Invitation, link: String) -> InviteSummary<'_> {
    InviteSummary {
        id: Cow::Borrowed(invite.id.as_str()),
        email: Cow::Borrowed(&invite.email),
        role: Cow::Borrowed(invite.role.wire()),
        expires_at: invite.expires_at_ms,
        created_at: invite.created_at_ms,
        link: Cow::Owned(link),
        email_status: Cow::Borrowed(invite.email_status.wire()),
        email_sent_at: invite.email_sent_at_ms,
    }
}

/// `{dashboard}/invites/{invite_id}`, the page that accepts `invite`.
pub(super) fn invite_link(dashboard: &Dashboard, invite: &Uuid7) -> String {
    dashboard.page([INVITES_PATH, invite.as_str()]).into()
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "test module: an unmet precondition should fail the test loudly"
)]
mod tests {
    use afd_connector::Dashboard;
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
            let dashboard = Dashboard::parse(base).expect("a URL base");
            let link = invite_link(&dashboard, &invite);
            assert!(
                link.ends_with(&format!("/invites/{INVITE}")),
                "{base} gave {link}"
            );
            assert!(!link.contains("//invites"), "{base} gave {link}");
        }
    }
}
