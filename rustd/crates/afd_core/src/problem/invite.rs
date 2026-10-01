//! What a client is told when an invite or a membership change is refused.
//!
//! Every refusal here is the caller's to act on, and the sentences name the
//! next step. None names an address: an invite link can be forwarded, and the
//! person holding it is not owed the invitee's email.

use super::Problem;
use crate::error_code;

/// This family's entries, in `REGISTRY` order.
pub(super) const INVITE: &[Problem] = &[
    Problem {
        code: error_code::INVITE_NOT_FOUND,
        status: 404,
        title: "Invite not found",
        hint: "The invite expired, was revoked, or was already accepted. Ask the account owner for a new one.",
        user_message: Some(
            "This invite is no longer valid. Ask the account owner to send a new one.",
        ),
    },
    Problem {
        code: error_code::INVITE_EMAIL_MISMATCH,
        status: 403,
        title: "Invite is for another address",
        hint: "Sign in with the email address the invite was sent to, then open the invite again.",
        user_message: Some(
            "This invite was sent to a different email address. Sign in with that address to accept it.",
        ),
    },
    Problem {
        code: error_code::INVITE_CONFLICT,
        status: 409,
        title: "Already invited or a member",
        hint: "The address has a pending invite or already belongs to the account. Revoke the pending invite to send a new one.",
        user_message: Some("That person already has an invite or is already a member."),
    },
    Problem {
        code: error_code::MEMBER_LAST_OWNER,
        status: 409,
        title: "Account needs an owner",
        hint: "An account keeps at least one owner. The last owner cannot be removed.",
        user_message: Some("The account's only owner cannot be removed."),
    },
    Problem {
        code: error_code::INVITE_EMAIL_UNAVAILABLE,
        status: 503,
        title: "Invite email could not be sent",
        hint: "Email is not set up, or the mail relay refused or did not answer. The invite is still valid: copy its link, or send again later.",
        user_message: Some(
            "We could not send the email. The invite still works: copy its link and send it yourself.",
        ),
    },
];
