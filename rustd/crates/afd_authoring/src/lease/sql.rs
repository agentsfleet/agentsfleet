//! Guard reads acquire wallet, current subject, draft and attempt in that order.

pub(super) const FIND: &str = "\
SELECT to_jsonb(a)::text FROM fleet.authoring_attempts a
WHERE lease_id=$1::uuid AND runner_id=$2::uuid";

pub(crate) const WALLET: &str = "\
SELECT tenant_id::text FROM billing.tenant_wallet WHERE tenant_id=$1::uuid FOR UPDATE";

pub(super) const SUBJECT: &str = "\
SELECT oidc_subject FROM core.users WHERE id=$1::uuid";

pub(super) const LOCK: &str = "\
SELECT to_jsonb(a)::text FROM fleet.authoring_attempts a
WHERE lease_id=$1::uuid AND runner_id=$2::uuid FOR UPDATE";

pub(super) use crate::sql::CLOCK;

pub(super) const MESSAGE: &str = "\
SELECT EXISTS(SELECT 1 FROM core.authoring_messages
WHERE draft_id=$1::uuid AND sequence=$2 AND state=$3)";
