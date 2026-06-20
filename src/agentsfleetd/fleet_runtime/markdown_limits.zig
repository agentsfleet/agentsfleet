/// Shared caps for raw `SKILL.md` and `TRIGGER.md` bodies. The bodies ride the
/// install request, the stored agent row, and runner leases, so import/create/
/// patch must agree on one ceiling.
pub const MAX_SOURCE_LEN: usize = 64 * 1024; // 65,536 bytes
pub const MAX_TRIGGER_LEN: usize = 64 * 1024; // 65,536 bytes
