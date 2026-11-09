//! The CLI device-login session's statements (`docs/AUTH_DEVICE_LOGIN.md`).
//!
//! Every status value is bound from `SessionStatus`, never written here (STS).
//! Deadlines come from `clock_timestamp()` plus a bound interval; non-locking
//! reads and the sweep compare against `statement_timestamp()`.

/// Opens a `pending` session. `$1` id · `$2` status · `$3` CLI public key ·
/// `$4` token name · `$5` time to live.
pub const INSERT_SESSION: &str = include_str!("session/insert_session.sql");

/// A live session's poll view. `$1` id.
pub const SELECT_SESSION: &str = include_str!("session/select_session.sql");

/// Locks a session for verification, then reads the clock once (SQL-LOCKCLOCK).
/// `$1` id.
pub const LOCK_SESSION: &str = include_str!("session/lock_session.sql");

/// Records a wrong code: the attempt count, and an abort at the cap. `$1` id ·
/// `$2` attempts · `$3` status · `$4` abort reason · `$5` time to live.
pub const RECORD_ATTEMPT: &str = include_str!("session/record_attempt.sql");

/// Consumes a session at the locked clock reading. `$1` id · `$2` status ·
/// `$3` the locked clock · `$4` replay window · `$5` fingerprint · `$6` time to live.
pub const CONSUME_SESSION: &str = include_str!("session/consume_session.sql");

/// Approves a live `pending` session in one statement. `$1` id · `$2` pending ·
/// `$3` verification pending · `$4` dashboard key · `$5` ciphertext · `$6` nonce ·
/// `$7` code digest · `$8` approver · `$9` time to live.
pub const APPROVE_SESSION: &str = include_str!("session/approve_session.sql");

/// A live session's status, to classify a zero-row approval. `$1` id.
pub const SELECT_SESSION_STATUS: &str = include_str!("session/select_session_status.sql");

/// Aborts every live session one approver owns. `$1` approver · `$2` terminal
/// statuses · `$3` aborted · `$4` reason · `$5` time to live.
pub const ABORT_OWNED_SESSIONS: &str = include_str!("session/abort_owned_sessions.sql");
