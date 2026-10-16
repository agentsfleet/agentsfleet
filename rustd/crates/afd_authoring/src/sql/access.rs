//! The authenticated subject must still belong to the workspace owner's account.

/// `$1` workspace, `$2` authenticated identity-provider subject.
pub(crate) const PERSON: &str = "\
SELECT u.id::text, w.tenant_id::text, u.tenant_id = w.tenant_id AS personal_owner
FROM core.users u JOIN core.workspaces w ON w.id = $1::uuid
WHERE u.oidc_subject = $2
FOR SHARE OF u, w";

/// Joined membership is locked before touching private state; a committed
/// revocation wins before replay, and a later revocation waits for this request.
pub(crate) const MEMBER: &str = "\
SELECT id::text FROM core.memberships
WHERE tenant_id = $1::uuid AND user_id = $2::uuid FOR SHARE";
