//! Centralized SQL for per-user, per-workspace dashboard UI preference reads
//! and writes.

/// The Clerk subject on the principal is an opaque external string; every
/// prefs row keys on the internal `core.users.id` it maps to. The constant name
/// keeps `USER_ID` because that is what the CALLER binds it as — on
/// `core.user_preferences.user_id`, a real parent key. Only the column read
/// here was renamed.
pub const SELECT_USER_ID_BY_SUBJECT =
    \\SELECT id::text
    \\FROM core.users
    \\WHERE oidc_subject = $1
;

pub const SELECT_BAG =
    \\SELECT pref_key, pref_value
    \\FROM core.user_preferences
    \\WHERE user_id = $1::uuid AND workspace_id = $2::uuid
    \\ORDER BY pref_key
;

pub const UPSERT_PREF =
    \\INSERT INTO core.user_preferences
    \\  (id, user_id, workspace_id, pref_key, pref_value, created_at, updated_at)
    \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $6)
    \\ON CONFLICT (user_id, workspace_id, pref_key) DO UPDATE SET
    \\  pref_value = EXCLUDED.pref_value,
    \\  updated_at = EXCLUDED.updated_at
;
