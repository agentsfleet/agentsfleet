//! What a suite needs to prove a provider call against a real socket, in
//! this crate and in the crates that embed it: a TLS listener a fake provider
//! serves on, and a client that trusts only it.

pub mod tls;
