//! The target a profile selected and the addresses that decision admits.

use std::process::Command;

use url::{Host, Url};

use super::{DATABASE_ENDPOINT, Profile, REDIS_ENDPOINT, TARGET_VARIABLE};
use crate::error::{Error, Result};

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
    let configured = endpoint_port(surface, endpoint)?;
    let output = Command::new("docker")
        .args(["compose", "port", service, container_port])
        .output()
        .map_err(|source| Error::RigIdentityUnavailable { service, source })?;
    let published = output
        .status
        .success()
        .then(|| published_port(&output.stdout))
        .flatten();
    if published != Some(configured) {
        return Err(Error::RigIdentityUnverified { surface, service });
    }
    Ok(())
}

fn endpoint_port(surface: &'static str, raw: &str) -> Result<u16> {
    Url::parse(raw)
        .ok()
        .and_then(|url| url.port_or_known_default())
        .ok_or_else(|| Error::UnsafeTarget {
            surface,
            address: "portless".to_owned(),
        })
}

fn published_port(raw: &[u8]) -> Option<u16> {
    core::str::from_utf8(raw)
        .ok()?
        .trim()
        .rsplit_once(':')?
        .1
        .parse()
        .ok()
}

/// Require one URL to name loopback, using the same parser for every scheme.
fn local_endpoint(surface: &'static str, raw: &str) -> Result<()> {
    let parsed = Url::parse(raw).map_err(|_source| Error::UnsafeTarget {
        surface,
        address: "unparseable".to_owned(),
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
