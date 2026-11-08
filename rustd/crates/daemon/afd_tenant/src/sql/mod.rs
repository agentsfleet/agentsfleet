//! Every statement this crate runs, collected, and nothing else.
//!
//! Split by DOMAIN rather than by line count, for the reason `afd_fleet::sql`
//! records: Rust has real modules, so no file needs a re-export to stay
//! findable and `grep -rn 'SELECT' src/sql/` still returns everything.

/// The column a malformed identifier reports against, wherever a reader
/// meets it.
pub(crate) const COLUMN_ID: &str = "id";

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
    /// The account's name, as every statement that shows one resolves it: the
    /// owner's display name when one is stored, else the account's own name,
    /// which signup takes from the owner's address. The owner role's spelling
    /// is bound, never written here (RULE STS).
    const OWNER_NAME: &str = "COALESCE(owner.display_name, t.name) AS owner_name";
    const OWNER_JOIN: &str = "\
LEFT JOIN LATERAL (
    SELECT u.display_name FROM identity.memberships om
    JOIN identity.users u ON u.id = om.user_id
    WHERE om.tenant_id = t.id AND om.role = $";

    /// Four statements name an account, and each spells the rule whole so a
    /// `PREPARE` can check it; this holds them to one spelling.
    #[test]
    fn every_statement_naming_an_account_resolves_it_the_same_way() {
        for statement in [
            super::invite::BEGIN_EMAIL_ATTEMPT,
            super::invite::SELECT_PENDING_FOR_EMAIL,
            super::workspace::SELECT_SUBJECT_ACCOUNTS,
            super::workspace::SELECT_TENANT_ACCOUNT,
        ] {
            assert!(statement.contains(OWNER_NAME), "{statement}");
            assert!(statement.contains(OWNER_JOIN), "{statement}");
        }
    }
}
