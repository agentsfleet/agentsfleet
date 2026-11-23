//! Automatic quarantine keeps operator intent and run deadlines independent.

pub(crate) const REASON: &str = afd_api_wire::operator::CordonReason::MissedHeartbeats.as_str();
pub(crate) const CORDONED: &str = "runner_cordoned";
pub(crate) const LIFTED: &str = "runner_uncordoned";
