//! The statement a run's question is raised through.
//!
//! A run that ends asking the person raises a `question` gate on its turn,
//! and the person's answer lands as a new turn that resumes it
//! (`afd_approval`'s inbox). No rule holds an event for a person, so nothing
//! here decides whether a run may proceed.

pub mod sql;
