//! The bounds every API login runs under, set on the login role at provisioning
//! (`M242_001` §6).
//!
//! Postgres applies a role's settings only to the role that logs in, so each
//! API login carries them: `ALTER ROLE <login> SET lock_timeout = …,
//! statement_timeout = …`. The test lane's `_api_login` and the region-move
//! playbook set them; `the_api_login_runs_under_the_intake_bounds` reads them
//! back through the API pool, so the three spellings cannot drift. A
//! transaction that sets its own bounds with `set_config` still wins.

/// How long an API statement waits for a lock before failing with `55P03`.
pub const LOCK_TIMEOUT: &str = "2000ms";

/// How long an API statement runs before failing with `57014`.
pub const STATEMENT_TIMEOUT: &str = "5000ms";
