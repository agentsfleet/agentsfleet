//! Two shapes a fleet's row and grants take on their way to the wire.
//!
//! Both are conversions with one rule each that is not obvious from the types,
//! which is why they are here with their tests rather than inline in the
//! assembly: a reader checking "what may a fleet with nothing approved reach"
//! should find the answer beside the assertion that proves it.

use std::borrow::Cow;

use af_fleet_runtime::config::{Access, HttpEgress as Stored, RepositoryBinding as Approved};
use af_wire::policy::{HttpEgress, NetworkPolicy, RepositoryAccess, RepositoryBinding};
use serde_json::{Map, Value};

use crate::policy::egress::API_HOST;

/// The field of a static secret naming the one host it may be sent to.
const FIELD_HOST: &str = "host";

/// The fleet row's two network toggles, as a person set them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Network {
    /// `fleet.fleets.http_egress`: restricted to named hosts, or public.
    pub http_egress: Stored,
    /// `fleet.fleets.package_domains`: whether the package registries are
    /// reachable from a restricted fleet.
    pub package_domains: bool,
}

/// The fleet's network policy: its two toggles, and an `allow` list naming
/// exactly the hosts its approved grants send credentials to.
///
/// A fleet with nothing approved reaches no named host. Each approved static
/// secret contributes its own `host` field, the one host it is ever sent to,
/// and an approved GitHub scope contributes the API host its origin rules
/// govern. A secret with no readable `host` contributes nothing.
pub(super) fn network(
    toggles: Network,
    statics: &Map<String, Value>,
    github: bool,
) -> NetworkPolicy<'static> {
    let hosts = statics
        .values()
        .filter_map(|stored| stored.get(FIELD_HOST).and_then(Value::as_str))
        .chain(github.then_some(API_HOST));
    let allow = hosts.fold(Vec::new(), |mut allow: Vec<Cow<'static, str>>, host| {
        if !allow.iter().any(|named| named.eq_ignore_ascii_case(host)) {
            allow.push(Cow::Owned(host.to_owned()));
        }
        allow
    });
    NetworkPolicy {
        http_egress: match toggles.http_egress {
            Stored::Restricted => HttpEgress::Restricted,
            Stored::Public => HttpEgress::Public,
        },
        package_manager_domains: toggles.package_domains,
        allow,
    }
}

/// The binding a run is TOLD, from the scope a person APPROVED.
///
/// The `match` is exhaustive, so a new access level is a compile error here
/// rather than a value silently dropped on the way to the runner — which is
/// what makes the runner's refusal and the mint's scoping agree about one
/// binding. Owned, because the binding is parsed per run from the grant row.
pub(super) fn wire_binding(approved: &Approved) -> RepositoryBinding<'static> {
    RepositoryBinding {
        repositories: approved
            .repositories()
            .iter()
            .map(|name| Cow::Owned(name.to_string()))
            .collect(),
        access: match approved.access() {
            Access::Read => RepositoryAccess::Read,
            Access::Write => RepositoryAccess::Write,
        },
        // A read binding opens no Pull Request, so it has no base to name. The
        // wire field is not optional, and empty is how "none" is spelled there.
        base_branch: Cow::Owned(approved.reach().base().unwrap_or_default().to_owned()),
    }
}

#[cfg(test)]
mod tests;
