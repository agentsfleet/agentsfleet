//! SQL statement text for the Slack connector domain (RULE SQLMOD — query text
//! lives here, grepable in one place).

/// Which fleet, if any, a Slack channel is bound to. The triple
/// `(provider, external_account_id, external_channel_id)` is the natural key —
/// a channel id alone is not unique across Slack workspaces.
pub const SELECT_CHANNEL_FLEET =
    \\SELECT fleet_id::text FROM core.connector_channels
    \\WHERE provider = $1 AND external_account_id = $2 AND external_channel_id = $3
;

/// Bind a channel to a fleet. `DO NOTHING` on the natural key makes a repeated
/// bind idempotent rather than re-pointing an existing channel.
pub const INSERT_CHANNEL_BINDING =
    \\INSERT INTO core.connector_channels
    \\  (uid, provider, external_account_id, external_channel_id, fleet_id, kind, created_at)
    \\VALUES ($1::uuid, $2, $3, $4, $5::uuid, $6, $7)
    \\ON CONFLICT (provider, external_account_id, external_channel_id) DO NOTHING
;

pub const SELECT_FLEET_BY_NAME =
    \\SELECT id::text FROM core.fleets WHERE workspace_id = $1::uuid AND name = $2
;
