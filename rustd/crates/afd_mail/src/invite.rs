//! The invite email: what it says, rendered from `templates/invite.{html,txt}`.
//!
//! Three variables and no more, because an invite is issued knowing only the
//! invitee's address: who invited them, which account, and the link. Askama
//! checks the variables against these structs at compile time and escapes every
//! one in the HTML part, so a display name carrying markup renders as text.

use askama::Template;

use crate::Result;

/// What follows an owner's name when the account is named after them.
///
/// The dashboard spells the same label in `accountLabel`
/// (`ui/packages/app/components/layout/workspace-groups.ts`), so the email and
/// the invites page name the account identically.
pub const ACCOUNT_LABEL_SUFFIX: &str = "'s account";

/// The subject line, around the account's label.
const SUBJECT_PREFIX: &str = "You're invited to join ";
const SUBJECT_SUFFIX: &str = " on agentsfleet";

/// Who is inviting, into whose account, and where to accept.
#[derive(Debug, Clone, Copy)]
pub struct InviteLetter<'a> {
    /// The inviter's display name, or their address when they have none.
    pub inviter_name: &'a str,
    /// The name the account goes by: its owner's display name, or the
    /// account's own name — the `owner_name` every account label is built from.
    pub owner_name: &'a str,
    /// The dashboard page that accepts this invite.
    pub invite_url: &'a str,
}

/// One email, rendered and ready to address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedEmail {
    /// The subject line.
    pub subject: String,
    /// The HTML part.
    pub html: String,
    /// The plain-text part, for clients that do not render HTML.
    pub text: String,
}

#[derive(Template)]
#[template(path = "invite.html")]
struct InviteHtml<'a> {
    inviter_name: &'a str,
    account_name: &'a str,
    invite_url: &'a str,
}

#[derive(Template)]
#[template(path = "invite.txt")]
struct InviteText<'a> {
    inviter_name: &'a str,
    account_name: &'a str,
    invite_url: &'a str,
}

/// The label an account goes by, as the dashboard prints it: "John's account".
#[must_use]
pub fn account_label(owner_name: &str) -> String {
    format!("{owner_name}{ACCOUNT_LABEL_SUFFIX}")
}

/// Renders the invite's subject and both parts.
///
/// # Errors
/// Reports a template that would not render, which is this build's fault.
pub fn render_invite(letter: &InviteLetter<'_>) -> Result<RenderedEmail> {
    let account_name = account_label(letter.owner_name);
    let html = InviteHtml {
        inviter_name: letter.inviter_name,
        account_name: &account_name,
        invite_url: letter.invite_url,
    }
    .render()?;
    let text = InviteText {
        inviter_name: letter.inviter_name,
        account_name: &account_name,
        invite_url: letter.invite_url,
    }
    .render()?;
    Ok(RenderedEmail {
        subject: format!("{SUBJECT_PREFIX}{account_name}{SUBJECT_SUFFIX}"),
        html,
        text,
    })
}

#[cfg(test)]
mod tests;
