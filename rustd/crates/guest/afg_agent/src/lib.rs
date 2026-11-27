//! The in-VM agent: processes and files inside one sandbox, served to the
//! host over one connection.
//!
//! `agentsfleet-guest` runs this as a microVM's first process and serves the
//! host's vsock connection with it; the debug engine on a host without a
//! microVM serves it in-process on a Unix socket. The messages it reads and
//! answers are `af_guest_wire`'s, read as untrusted. It links nothing of the
//! runner's: a host crate reaches it only in a test or through the debug
//! engine, which the shipped runner does not carry.
//!
//! [`bind`] claims a socket path, and [`streamed`] takes a connected byte
//! stream, a virtual machine's vsock connection; [`Server::serve`] serves that
//! one connection, owning every process it starts. Taking the link is split
//! from serving so a caller takes it before it confines itself.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

mod server;

pub use self::server::{
    Entry, InnerEnvironment, LeafFreezer, Server, Tenant, bind, serve, streamed,
};
