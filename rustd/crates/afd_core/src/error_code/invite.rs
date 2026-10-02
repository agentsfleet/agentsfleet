//! The codes an invite into an account, and the members it makes, answer with.
//!
//! `UZ-INV-*`. An owner invites an address, the person holding it accepts,
//! and the owner may later remove them. Each refusal here is one a person can
//! act on: open the right invite, sign in with the right address, stop
//! inviting someone already in, keep an owner in the account.

use super::ErrorCode;

/// No invite that can still be accepted carries that id.
///
/// One answer for never issued, expired, revoked, and already accepted by
/// somebody else, so the accept route is no oracle for which invites exist.
pub const INVITE_NOT_FOUND: ErrorCode = ErrorCode::declare("UZ-INV-001");

/// The invite was sent to a different address than the signed-in account's.
///
/// A 403 rather than a 404: the caller proved they hold the link, and the
/// remedy is to sign in with the address it was sent to. The refusal never
/// names that address, so a forwarded link reveals nothing about its invitee.
pub const INVITE_EMAIL_MISMATCH: ErrorCode = ErrorCode::declare("UZ-INV-002");

/// The address already has a pending invite, or already belongs to the account.
pub const INVITE_CONFLICT: ErrorCode = ErrorCode::declare("UZ-INV-003");

/// Removing this member would leave the account with no owner.
pub const MEMBER_LAST_OWNER: ErrorCode = ErrorCode::declare("UZ-INV-004");

/// The invite exists but its email could not be sent: the relay is not set up,
/// or it refused or did not answer.
///
/// Answered only by the send-again route; creating an invite never fails on
/// email. The invite stays acceptable, so the owner can copy its link instead.
pub const INVITE_EMAIL_UNAVAILABLE: ErrorCode = ErrorCode::declare("UZ-INV-005");
