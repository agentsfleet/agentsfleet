//! What a caller is told when the runner plane refuses, apart from `ErrorKind`,
//! which says what happened. Each sentence is client-visible.

/// The sentence an out-of-bounds host identifier earns.
pub const DETAIL_HOST_ID_BOUNDS: &str = "host_id must be 1-256 chars";

/// The sentence a malformed registry allowlist earns.
pub const DETAIL_REGISTRY_ALLOWLIST: &str = "registry_allowlist entries must be host[:port] names";
/// The optional selection uses the same canonical identity as the catalogue.
pub const DETAIL_DECISION_MODEL_ID: &str =
    "decision_model_id must be a canonical version-7 UUID or null";

/// The sentence a vanished runner earns.
pub const DETAIL_RUNNER_NOT_FOUND: &str = "runner not found";

/// The refusal a self-test ask earns when its runner is terminal.
pub const DETAIL_SELFTEST_REFUSED: &str = "revoked runners cannot be asked to self-test";
/// The refusal a delete of a runner still in service earns.
pub const DETAIL_RUNNER_NOT_REVOKED: &str = "active runner must be revoked before deletion";
/// The refusal a delete of a runner still holding a live or unbilled run earns.
pub const DETAIL_RUNNER_STILL_RUNNING: &str =
    "runner still holds a live or unbilled run; retry once it is released and billed";
/// The refusal a registration key reused for a different request earns.
pub const DETAIL_REGISTRATION_KEY_REUSED: &str =
    "this Idempotency-Key already registered a runner with a different request";
/// The refusal a platform caller rotating a tenant runner's token earns.
pub const DETAIL_TENANT_MANAGED: &str = "a tenant runner's token is rotated by its tenant";
