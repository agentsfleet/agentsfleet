//! The statements an invite into an account is issued, read, revoked and
//! accepted by.
//!
//! `email` is lowercased before it is bound, so every comparison here is plain
//! equality and the partial indexes in `schema/923_workspace_invites.sql`
//! serve it.

/// Whether an address already belongs to the account.
///
/// `$1` tenant · `$2` the lowercased address. The stored address is lowercased
/// here because users are written as the identity provider spelled them; an
/// account's members are few, so this walks their rows by the tenant prefix of
/// `uq_memberships_tenant_id_user_id` and reads each user by primary key.
pub const SELECT_MEMBER_BY_EMAIL: &str = "\
SELECT 1 FROM core.memberships m \
JOIN core.users u ON u.id = m.user_id \
WHERE m.tenant_id = $1::uuid AND lower(u.email) = $2 \
LIMIT 1";

/// Retires an expired pending invite for the address, so a new one can be
/// issued under the one-pending-invite index.
///
/// `$1` tenant · `$2` address · `$3` now. Expired but unaccepted invites stay
/// pending as far as that index can tell, since its predicate cannot read the
/// clock; this is what lets them go.
pub const REVOKE_EXPIRED_PENDING: &str = "\
UPDATE core.invites SET revoked_at = $3, updated_at = $3 \
WHERE tenant_id = $1::uuid AND email = $2 \
  AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at <= $3";

/// One invite.
///
/// `$1` id · `$2` tenant · `$3` address · `$4` role · `$5` inviter · `$6`
/// expiry · `$7` now · `$8` sends attempted so far. `uq_invites_tenant_id_email_pending`
/// refuses a second pending invite for the same address.
pub const INSERT_INVITE: &str = "\
INSERT INTO core.invites \
  (id, tenant_id, email, role, invited_by, expires_at, email_attempts, created_at, updated_at) \
VALUES ($1::uuid, $2::uuid, $3, $4, $5::uuid, $6, $8, $7, $7)";

/// The account's invites that can still be accepted, newest first.
///
/// `$1` tenant · `$2` now.
pub const SELECT_TENANT_PENDING: &str = concat!(
    select_invitation!(),
    "FROM core.invites \
     WHERE tenant_id = $1::uuid AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at > $2 \
     ORDER BY invites.created_at DESC, invites.id DESC"
);

/// Revokes one pending invite.
///
/// `$1` tenant · `$2` invite · `$3` now. Scoped by tenant in the statement, so
/// an owner cannot revoke another account's invite by guessing its id. No row
/// means it was already revoked, accepted, or never this account's, and the
/// caller answers all three the same way.
pub const REVOKE_INVITE: &str = "\
UPDATE core.invites SET revoked_at = $3, updated_at = $3 \
WHERE tenant_id = $1::uuid AND id = $2::uuid \
  AND accepted_at IS NULL AND revoked_at IS NULL";

/// What is waiting for an address, with the account each invite is for.
///
/// `$1` the lowercased address · `$2` now · `$3` the owner role's spelling.
pub const SELECT_PENDING_FOR_EMAIL: &str = concat!(
    "SELECT i.id::text AS id, t.id::text AS tenant_id, \
            COALESCE(owner.display_name, t.name) AS owner_name, i.expires_at \
     FROM core.invites i \
     JOIN core.tenants t ON t.id = i.tenant_id ",
    owner_name_join!(3),
    "WHERE i.email = $1 AND i.accepted_at IS NULL AND i.revoked_at IS NULL \
       AND i.expires_at > $2 \
     ORDER BY i.created_at DESC, i.id DESC"
);

/// The invite an accept acts on, locked until the accept commits.
///
/// `$1` invite. The lock is what makes two tabs accepting at once one accept:
/// the second waits, then reads the first one's stamp.
pub const LOCK_INVITE: &str = concat!(
    select_invitation!(),
    "FROM core.invites WHERE id = $1::uuid FOR UPDATE"
);

/// The membership an accept writes. A second accept writes nothing.
///
/// `$1` id · `$2` tenant · `$3` user · `$4` role · `$5` now.
pub const INSERT_MEMBERSHIP: &str = "\
INSERT INTO core.memberships (id, tenant_id, user_id, role, created_at) \
VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5) \
ON CONFLICT (tenant_id, user_id) DO NOTHING";

/// Whether a person still holds a membership in the account.
///
/// `$1` tenant · `$2` user. Asked when an invite the caller already accepted is
/// accepted again: a replay answers as the first accept did, but only while
/// the membership stands, so a removed member cannot rejoin through an old link.
pub const SELECT_MEMBERSHIP_EXISTS: &str = "\
SELECT 1 FROM core.memberships WHERE tenant_id = $1::uuid AND user_id = $2::uuid";

/// Stamps an invite accepted.
///
/// `$1` invite · `$2` user · `$3` now.
pub const MARK_ACCEPTED: &str = "\
UPDATE core.invites SET accepted_at = $3, accepted_by = $2::uuid, updated_at = $3 \
WHERE id = $1::uuid";

/// The workspaces an account holds, oldest first: what an accept opened.
///
/// `$1` tenant.
pub const SELECT_TENANT_WORKSPACE_IDS: &str = "\
SELECT id::text FROM core.workspaces WHERE tenant_id = $1::uuid \
ORDER BY workspaces.created_at, workspaces.id";
