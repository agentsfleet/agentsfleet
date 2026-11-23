//! What a suite needs to prove a provider call, against a real socket or not.
//!
//! In this crate and in the crates that embed it: a TLS listener a fake
//! provider serves on, a client that trusts only it, the turn a fake
//! streams back, and the failures a turn ends with.

pub mod failure;
pub mod messages;
pub mod tls;
