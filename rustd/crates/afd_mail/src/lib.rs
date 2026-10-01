//! Product email for `agentsfleetd`: today, the invite.
//!
//! An invite is rendered from templates kept in this repository
//! (`templates/invite.{html,txt}`, askama) and sent over the Simple Mail
//! Transfer Protocol (SMTP) through the relay the `smtp-relay` platform bag
//! names in the admin workspace's vault. Resend is that relay today; any SMTP
//! relay plugs in by changing the bag.
//!
//! Rendering and delivery stay apart: [`render_invite`] is a pure function of
//! its three variables, and [`InviteMailer`] owns the relay, the deadline and
//! the retry. The caller — the invite route — commits the invite and its
//! attempt number first, so nothing here can lose an invite.

mod deliver;
mod error;
mod invite;
mod mailer;
mod relay;

pub use self::deliver::IDEMPOTENCY_HEADER;
pub use self::error::{Error, Result};
pub use self::invite::{
    ACCOUNT_LABEL_SUFFIX, INVITE_VALID_DAYS, InviteLetter, RenderedEmail, account_label,
    render_invite,
};
pub use self::mailer::{InviteMailer, InviteSend, MAIL_SEND_DEADLINE, Outcome, deliverable};
pub use self::relay::{SMTP_RELAY_BAG, SMTPS_PORT, Security};
