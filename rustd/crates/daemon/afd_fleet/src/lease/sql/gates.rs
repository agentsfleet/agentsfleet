//! The gates read: every fact the money and approval gates judge, in one
//! statement (M245 §5).

/// Spend, the recorded gate and its standing, and the approved integrations.
///
/// `$1` workspace · `$2` fleet · `$3` the rolling day · `$4` the event ·
/// `$5` the approved grant status.
pub const SELECT_GATES: &str = include_str!("select_gates.sql");
