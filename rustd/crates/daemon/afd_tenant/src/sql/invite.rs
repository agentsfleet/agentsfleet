//! The statements an invite into an account is issued, read, revoked and
//! accepted by.
//!
//! `email` is lowercased before it is bound, so every comparison here is plain
//! equality and the partial indexes in `schema/1140_identity_invites.sql`
//! serve it. An invite is pending while `resolved_at` is NULL; the statement
//! that resolves it stamps `resolved_at` and binds which way it went into
//! `status`, and each is guarded on `resolved_at IS NULL`, so an invite
//! resolves once. Each statement adds its own expiry comparison, since a
//! revoke reaches an expired invite and a list does not.

/// Whether an address already belongs to the account: a member, or a user
/// whose own account it is.
///
/// `$1` tenant · `$2` the lowercased address. The stored address is lowercased
/// here because users are written as the identity provider spelled them; an
/// account's members are few, so this walks their rows by the tenant prefix of
/// `uq_memberships_tenant_id_user_id` and reads each user by primary key. The
/// second arm finds the account's own user even without a membership row,
/// which access admits as owner; an invite would otherwise demote them.
pub const SELECT_MEMBER_BY_EMAIL: &str = include_str!("invite/select_member_by_email.sql");

/// Locks the address's pending invites until the issuing transaction commits.
///
/// `$1` tenant · `$2` address. Taken before [`SELECT_MEMBER_BY_EMAIL`], on the
/// same rows [`LOCK_INVITE`] holds while an accept runs: an accept in flight
/// makes the issue wait, its accepted row then drops out of this predicate,
/// and the member check that follows reads the membership it wrote. Without
/// it the check could miss that join and the accept would clear the
/// one-pending-invite index for an invite to someone already in.
pub const LOCK_PENDING_FOR_EMAIL: &str = include_str!("invite/lock_pending_for_email.sql");

/// Retires an expired pending invite for the address, so a new one can be
/// issued under the one-pending-invite index.
///
/// `$1` tenant · `$2` address · `$3` the expired status's spelling. Expired
/// but unresolved invites stay pending as far as that index can tell, since its
/// predicate cannot read the clock; this is what lets them go. It runs after
/// [`LOCK_PENDING_FOR_EMAIL`] in the same transaction, so the rows are already
/// held and `clock_timestamp()` is read after any wait for them.
pub const RETIRE_EXPIRED_PENDING: &str = include_str!("invite/retire_expired_pending.sql");

/// One invite, answering the deadline and the creation stamp it wrote.
///
/// `$1` id · `$2` tenant · `$3` address · `$4` role · `$5` the pending status's
/// spelling · `$6` inviter · `$7` how long it stays acceptable, an exact
/// interval. `uq_invites_tenant_id_email_pending` refuses a second pending
/// invite for the same address.
pub const INSERT_INVITE: &str = include_str!("invite/insert_invite.sql");

/// The account's invites that can still be accepted, newest first.
///
/// `$1` tenant. A non-locking read, so the deadline is compared against
/// `statement_timestamp()`.
pub const SELECT_TENANT_PENDING: &str = include_str!("invite/select_tenant_pending.sql");

/// Revokes one pending invite.
///
/// `$1` tenant · `$2` invite · `$3` the revoked status's spelling. Scoped by
/// tenant in the statement, so an owner cannot revoke another account's invite
/// by guessing its id. No row means it was already resolved or never this
/// account's; [`SELECT_JOINED_THROUGH_INVITE`] tells the accepted one apart.
pub const REVOKE_INVITE: &str = include_str!("invite/revoke_invite.sql");

/// Whether the person who accepted an invite still belongs to its account.
///
/// `$1` tenant · `$2` invite. Asked only after a revoke changed nothing, as
/// its own statement: an accept holding the invite's row lock makes the
/// revoke wait, and only a statement begun after that accept committed can
/// read the membership it wrote. The invite by primary key, the membership by
/// `uq_memberships_tenant_id_user_id`.
pub const SELECT_JOINED_THROUGH_INVITE: &str =
    include_str!("invite/select_joined_through_invite.sql");

/// What is waiting for an address, with the account each invite is for.
///
/// `$1` the lowercased address · `$2` the owner role's spelling. Non-locking,
/// so compared against `statement_timestamp()`.
pub const SELECT_PENDING_FOR_EMAIL: &str = include_str!("invite/select_pending_for_email.sql");

/// The invite an accept acts on, locked until the accept commits, and the
/// clock read after the lock.
///
/// `$1` invite. The lock is what makes two tabs accepting at once one accept:
/// the second waits, then reads the first one's stamp. `locked_at` is
/// `clock_timestamp()` read in a step that depends on the locked row, so an
/// invite that expired while this waited for it is judged expired.
pub const LOCK_INVITE: &str = include_str!("invite/lock_invite.sql");

/// The membership an accept writes. A second accept writes nothing.
///
/// `$1` id · `$2` tenant · `$3` user · `$4` role.
pub const INSERT_MEMBERSHIP: &str = include_str!("invite/insert_membership.sql");

/// Whether a person still holds a membership in the account.
///
/// `$1` tenant · `$2` user. Asked when an invite the caller already accepted is
/// accepted again: a replay answers as the first accept did, but only while
/// the membership stands, so a removed member cannot rejoin through an old link.
pub const SELECT_MEMBERSHIP_EXISTS: &str = include_str!("invite/select_membership_exists.sql");

/// Stamps an invite accepted, once.
///
/// `$1` invite · `$2` user · `$3` the accepted status's spelling.
pub const MARK_ACCEPTED: &str = include_str!("invite/mark_accepted.sql");

/// The workspaces an account holds, oldest first: what an accept opened.
///
/// `$1` tenant.
pub const SELECT_TENANT_WORKSPACE_IDS: &str =
    include_str!("invite/select_tenant_workspace_ids.sql");

/// Counts one more send of a pending invite, and reads what its email says.
///
/// `$1` tenant · `$2` invite · `$3` the owner role's spelling. One statement,
/// so the attempt is committed before the send it names, and a send-again can
/// never reuse an earlier attempt's idempotency key. The row is locked before
/// the clock is read, so an invite that expired during the wait is not
/// counted. No row means the invite is not this account's, or can no longer be
/// accepted. The inviter is named by display name, else by address: the email
/// has to say who it is from.
pub const BEGIN_EMAIL_ATTEMPT: &str = include_str!("invite/begin_email_attempt.sql");

/// Records what became of one send, unless a later send has begun since.
///
/// `$1` invite · `$2` the attempt this result is for · `$3` status · `$4`
/// whether the relay accepted it, which stamps `email_sent_at` with the
/// database clock. Guarded on the attempt, so two send-agains
/// racing each other leave the newer one's result, whichever finishes last.
pub const RECORD_EMAIL_RESULT: &str = include_str!("invite/record_email_result.sql");
