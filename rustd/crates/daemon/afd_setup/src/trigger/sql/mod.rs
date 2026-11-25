//! `fleet.triggers`, read and written on the caller's connection.

/// `$1` fleet.
pub(super) const LIST: &str = include_str!("list.sql");

/// `$1` id, `$2` fleet, `$3` workspace, `$4` trigger type, `$5` source, `$6` events,
/// `$7` repositories, `$8` verifier scheme, `$9` verifier header, `$10` secret
/// name, `$11` channel. No row when the fleet already holds the trigger type and source.
pub(super) const INSERT: &str = include_str!("insert.sql");

/// `$1` trigger, `$2` fleet, `$3` workspace. Answers the removed row's trigger type.
pub(super) const REMOVE: &str = include_str!("remove.sql");
