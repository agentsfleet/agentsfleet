//! What a suite needs to prove a provider call against a real socket.
//!
//! In this crate and in the crates that embed it: a TLS listener a fake
//! provider serves on, a client that trusts only it, and the turn a fake
//! streams back.

pub mod messages;
pub mod tls;
