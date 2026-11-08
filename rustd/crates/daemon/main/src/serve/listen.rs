//! The listening socket, and the supervised accept loop that drains it.
//!
//! Split from the boot sequence in [`super`] because the bind is the one part
//! of boot that is a deployment fact rather than an ordering fact: which
//! address the tunnel can reach, and why one socket serves both stacks.

use std::net::{Ipv6Addr, SocketAddr};

use tokio::net::TcpListener;

use super::accept::accept_loop;
use super::drain::Drain;
use crate::error::BootFailure;
use crate::supervisor::Supervisor;

/// Binds `[::]:port` — the address the Cloudflare Tunnel can actually reach.
///
/// # Why the address is a deployment contract, not a preference
///
/// The daemon publishes no public Fly service in any environment:
/// `deploy/fly/agentsfleetd-{dev,prod}/fly.toml` says so, and the only route in
/// is Cloudflare Tunnel dialling `agentsfleetd-<env>.internal:3000`. Fly's
/// private network resolves a `.internal` name to a 6PN address, which is
/// **IPv6 only** — so a listener bound to `0.0.0.0` refuses the tunnel's
/// connection and the edge answers 502 while the machine still reports healthy,
/// because Fly's readiness probe reaches port 3000 over IPv4. That asymmetry
/// hides the fault: every health check passes while the tunnel is refused.
///
/// # Why one bind serves both stacks
///
/// An `AF_INET6` socket accepts IPv4 through v4-mapped addresses unless
/// `IPV6_V6ONLY` is set, and Linux leaves that option off by default
/// (`net.ipv6.bindv6only` is 0). That default is what this bind relies on, and
/// it is what keeps Fly's IPv4 readiness probe answered by the same listener
/// the tunnel reaches over IPv6. `std` offers no way to set the option anyway:
/// it must be changed between `socket()` and `bind()`, and `bind` does both.
/// The assumption is not left to inspection — `tests/serve.rs` connects over
/// each stack in turn, on Linux in Continuous Integration (CI).
///
/// # Errors
/// Returns the `io::Error` from the bind: a port already held, or one this
/// process may not have.
pub async fn dual_stack_listener(port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind((Ipv6Addr::UNSPECIFIED, port)).await
}

/// Binds `port` and hands the listener to a supervised accept loop.
///
/// Answers the RESOLVED address, so a bind to port 0 reports the port the
/// kernel chose.
pub(super) async fn listen(
    port: u16,
    router: axum::Router,
    supervisor: &mut Supervisor,
    drain: Drain,
) -> Result<SocketAddr, BootFailure> {
    let listener = dual_stack_listener(port).await?;
    let address = listener.local_addr()?;
    // `token` is the supervisor's: the loop passes it to each connection as the
    // ABORT signal, which fires only after the drain's bound has expired.
    supervisor.spawn(ACCEPT_LOOP, move |token| {
        accept_loop(listener, router, drain, token)
    });
    Ok(address)
}

/// The supervised name of the accept loop.
pub const ACCEPT_LOOP: &str = "accept_loop";
