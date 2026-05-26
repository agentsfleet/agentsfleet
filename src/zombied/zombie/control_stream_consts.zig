//! Control-plane string constants — Redis commands, event types, and
//! flat key/value field names for the zombie:control stream. Split from
//! control_stream.zig so consumers reference a single source of truth
//! for the wire-format vocabulary.

// Redis command tokens
pub const REDIS_XGROUP = "XGROUP";
pub const REDIS_CREATE = "CREATE";
pub const REDIS_MKSTREAM = "MKSTREAM";
pub const REDIS_OK = "OK";
pub const REDIS_BUSYGROUP = "BUSYGROUP";

// Stream entry fields
pub const FIELD_TYPE = "type";
pub const FIELD_REASON = "reason";
pub const FIELD_STATUS = "status";
pub const FIELD_WORKSPACE_ID = "workspace_id";
pub const FIELD_ZOMBIE_ID = "zombie_id";
pub const FIELD_CONFIG_REVISION = "config_revision";

// Event-type tokens
pub const EVENT_ZOMBIE_CREATED = "zombie_created";
pub const EVENT_ZOMBIE_STATUS_CHANGED = "zombie_status_changed";
pub const EVENT_ZOMBIE_CONFIG_CHANGED = "zombie_config_changed";
pub const EVENT_WORKER_DRAIN_REQUEST = "worker_drain_request";

// Log scopes
pub const LOG_XADD_FAIL = "xadd_fail";
