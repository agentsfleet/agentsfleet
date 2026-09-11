//! The target a profile selected and the addresses that decision admits.

use std::net::{IpAddr, SocketAddr};
use std::process::Command;

use url::{Host, Url};

use super::{DATABASE_ENDPOINT, Profile, REDIS_ENDPOINT, TARGET_VARIABLE};
use crate::error::{Error, Result};

const UNPARSEABLE: &str = "unparseable";

/// The datastores a lane opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// The compose Postgres and Redis `make/test-infra.mk` starts.
    Rig,
    /// A deployed environment, reached at this address.
    Deployed {
        /// What [`super::TARGET_VARIABLE`] carried.
        address: String,
    },
}

impl Profile {
    /// Bind a profile decision to the actual datastore endpoints.
    ///
    /// The rig is the saturation profile. It may touch only loopback, whatever
    /// credentials are present and whatever `BENCH_TARGET` says. This runs
    /// before either client is constructed, so a rejected URL opens no socket.
    ///
    /// # Errors
    ///
    /// [`Error::UnsafeTarget`] when a rig endpoint is malformed, hostless, or
    /// points anywhere except loopback.
    pub fn check_endpoints(
        self,
        target: &Target,
        database_url: &str,
        redis_url: &str,
    ) -> Result<()> {
        if matches!(target, Target::Deployed { .. }) {
            return Err(Error::UnsafeTarget {
                surface: TARGET_VARIABLE,
                address: "remote targets remain disabled until deployment identity is verified"
                    .to_owned(),
            });
        }
        local_endpoint(DATABASE_ENDPOINT, database_url)?;
        local_endpoint(REDIS_ENDPOINT, redis_url)
    }
}

impl Target {
    /// Prove each loopback endpoint is published by this worktree's compose rig.
    ///
    /// Loopback alone is insufficient: an SSH tunnel can forward a local port
    /// to a shared datastore. `docker compose port` binds the port to the
    /// running service in the current repository's compose project before any
    /// datastore client opens it.
    ///
    /// # Errors
    ///
    /// Refuses a missing service, a port mismatch, or a Docker invocation that
    /// cannot start.
    pub fn verify_owned_rig(&self, database_url: &str, redis_url: &str) -> Result<()> {
        if !matches!(self, Self::Rig) {
            return Err(Error::RigIdentityUnverified {
                surface: TARGET_VARIABLE,
                service: "remote",
            });
        }
        verify_service(DATABASE_ENDPOINT, database_url, "postgres", "5432")?;
        verify_service(REDIS_ENDPOINT, redis_url, "redis", "6380")
    }

    /// Whether every connected and discovered server must be loopback.
    #[must_use]
    pub const fn requires_loopback(&self) -> bool {
        matches!(self, Self::Rig)
    }

    /// Check a host reported by a connected server or topology response.
    ///
    /// # Errors
    ///
    /// [`Error::UnsafeTarget`] when a local-rig run reports a non-loopback
    /// server. Deployed profiles retain their bounded profile caps.
    pub fn check_discovered_host(&self, surface: &'static str, host: &str) -> Result<()> {
        if self.requires_loopback() && !is_loopback(host) {
            return Err(Error::UnsafeTarget {
                surface,
                address: host.to_owned(),
            });
        }
        Ok(())
    }
}

fn verify_service(
    surface: &'static str,
    endpoint: &str,
    service: &'static str,
    container_port: &'static str,
) -> Result<()> {
    let configured = endpoint_socket(surface, endpoint)?;
    let output = Command::new("docker")
        .args(["compose", "port", service, container_port])
        .output()
        .map_err(|source| Error::RigIdentityUnavailable { service, source })?;
    let published = output.status.success()
        && published_sockets(&output.stdout).any(|binding| binding_covers(binding, configured));
    if !published {
        return Err(Error::RigIdentityUnverified { surface, service });
    }
    Ok(())
}

fn endpoint_socket(surface: &'static str, raw: &str) -> Result<SocketAddr> {
    let parsed = Url::parse(raw).map_err(|_source| unsafe_address(surface, UNPARSEABLE))?;
    let ip: IpAddr = parsed
        .host_str()
        .and_then(|host| host.trim_matches(['[', ']']).parse().ok())
        .ok_or_else(|| unsafe_address(surface, "non-literal"))?;
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| unsafe_address(surface, "portless"))?;
    Ok(SocketAddr::new(ip, port))
}

fn published_sockets(raw: &[u8]) -> impl Iterator<Item = SocketAddr> + '_ {
    core::str::from_utf8(raw)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.parse().ok())
}

fn binding_covers(binding: SocketAddr, configured: SocketAddr) -> bool {
    binding.port() == configured.port()
        && binding.is_ipv4() == configured.is_ipv4()
        && (binding.ip().is_unspecified() || binding.ip() == configured.ip())
}

/// Require one URL to name loopback, using the same parser for every scheme.
fn local_endpoint(surface: &'static str, raw: &str) -> Result<()> {
    let parsed = Url::parse(raw).map_err(|_source| Error::UnsafeTarget {
        surface,
        address: UNPARSEABLE.to_owned(),
    })?;
    let host = parsed.host().ok_or_else(|| Error::UnsafeTarget {
        surface,
        address: "hostless".to_owned(),
    })?;
    let local = match host {
        Host::Domain(name) => is_loopback(name),
        Host::Ipv4(address) => address.is_loopback(),
        Host::Ipv6(address) => address.is_loopback(),
    };
    if !local {
        return Err(Error::UnsafeTarget {
            surface,
            address: host.to_string(),
        });
    }
    Ok(())
}

fn unsafe_address(surface: &'static str, address: &str) -> Error {
    Error::UnsafeTarget {
        surface,
        address: address.to_owned(),
    }
}

/// Whether a server-reported host is loopback.
fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

#[cfg(test)]
mod tests;
