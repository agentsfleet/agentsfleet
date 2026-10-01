//! Every statement this crate runs, collected, and nothing else.
//!
//! Split by DOMAIN rather than by line count, for the reason `afd_fleet::sql`
//! records: Rust has real modules, so no file needs a re-export to stay
//! findable and `grep -rn 'SELECT' src/sql/` still returns everything.
//!
//! The statements are byte-identical to their Zig originals. Row-equivalence is
//! the cutover invariant, so a statement is copied rather than re-derived;
//! where a `$n` order looks odd, it is odd in the original too.

/// The select list every invitation read opens with, in the names
/// `Invitation::read` reads them by, so a list and the accept lock cannot drift
/// apart. A macro because `concat!` takes literals, not constants.
macro_rules! select_invitation {
    () => {
        "SELECT id::text AS id, tenant_id::text AS tenant_id, email, role, expires_at, \
         created_at, accepted_at, accepted_by::text AS accepted_by, revoked_at, email_status, \
         email_sent_at "
    };
}

/// The account a row belongs to, named by its owner.
///
/// Joined against a `core.tenants` row aliased `t`: the owner's display name
/// when one is stored, else the account's own name, which signup takes from the
/// owner's address. `$slot` is the number of the parameter the owner role's
/// spelling is bound to, bound rather than written here (RULE STS). The lateral reads
/// `uq_memberships_tenant_id_user_id` by its tenant prefix: an account has a
/// handful of members, so this is a few index entries per account.
macro_rules! owner_name_join {
    ($slot:literal) => {
        concat!(
            "LEFT JOIN LATERAL ( \
               SELECT u.display_name FROM core.memberships om \
               JOIN core.users u ON u.id = om.user_id \
               WHERE om.tenant_id = t.id AND om.role = $",
            $slot,
            " AND u.display_name IS NOT NULL \
               ORDER BY om.created_at, om.id LIMIT 1 \
             ) owner ON TRUE "
        )
    };
}

/// The account's name as `owner_name_join!` resolves it, aliased
/// [`COLUMN_OWNER_NAME`]: the owner's display name when one is stored, else
/// the account's own name. Spelled once beside the join it reads, so the
/// naming rule cannot drift between the statements that use it.
macro_rules! owner_name_column {
    () => {
        "COALESCE(owner.display_name, t.name) AS owner_name"
    };
}

/// The column `owner_name_column!` names, as the readers fetch it.
pub(crate) const COLUMN_OWNER_NAME: &str = "owner_name";

pub mod apikey;
pub mod cli_credential;
pub mod invite;
pub mod member;
pub mod models;
pub mod preference;
pub mod signup;
pub mod workspace;

#[cfg(test)]
mod tests {
    /// Readers fetch the alias by `COLUMN_OWNER_NAME`; the macro must alias
    /// the projection to exactly that name.
    #[test]
    fn the_owner_name_projection_aliases_the_column_readers_fetch() {
        let projection = owner_name_column!();
        assert!(
            projection.ends_with(&format!(" AS {}", super::COLUMN_OWNER_NAME)),
            "{projection}"
        );
    }
}
