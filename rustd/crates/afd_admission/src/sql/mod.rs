//! Statements owned by the admissions ledger; explicit parameters at each caller.

pub(crate) const INSERT_ADMISSION: &str = include_str!("insert_admission.sql");
pub(crate) const FLEET_BUDGET: &str = include_str!("fleet_budget.sql");
pub(crate) const SELECT_BACKLOG: &str = include_str!("select_backlog.sql");
pub(crate) const SELECT_REPEATED: &str = include_str!("select_repeated.sql");
pub(crate) const SELECT_REPEATED_ON_FLEET: &str = include_str!("select_repeated_on_fleet.sql");
pub(crate) const SELECT_INSTALLED_POLICY: &str = include_str!("select_installed_policy.sql");
/// The report reads the admitted reply destination on its existing transaction.
pub const SELECT_REPLY_DESTINATION: &str = include_str!("select_reply_destination.sql");
