//! Native runner listeners require a complete identity before boot opens any socket.

use std::net::{Ipv6Addr, SocketAddr};

use af_core::env::{EnvSource, optional};

use super::{Fault, read::required};

/// Private TCP and UDP address shared by the two runner carriers.
pub const LISTEN_ADDR: &str = "AGENTSFLEET_RUNNER_STREAM_LISTEN_ADDR";
/// Certificate chain authenticating the daemon; key exchange is configured by `af_stream::tls`.
pub const TLS_CERT_FILE: &str = "AGENTSFLEET_RUNNER_STREAM_TLS_CERT_FILE";
/// Protected leaf private key. The issuer signing key is never delivered to runtime.
pub const TLS_KEY_FILE: &str = "AGENTSFLEET_RUNNER_STREAM_TLS_KEY_FILE";

const DETAIL_ADDRESS: &str =
    "use an IP address and port, for example [::]:4443, for the private TCP and UDP listeners";
const DETAIL_CERTIFICATE: &str = "set the path to the daemon's PEM certificate chain";
const DETAIL_PRIVATE_KEY: &str = "set the path to the daemon's protected PEM leaf private key";

/// Parsed deployment inputs; file contents are validated by the native listener.
#[derive(Clone, Debug)]
pub struct RunnerListenerConfig {
    pub(crate) address: SocketAddr,
    pub(crate) identity: af_stream::tls::Identity,
}

pub(super) fn read<E: EnvSource + ?Sized>(
    env: &E,
    faults: &mut Vec<Fault>,
) -> Option<RunnerListenerConfig> {
    let address = match optional(env, LISTEN_ADDR) {
        None => Some(SocketAddr::from((Ipv6Addr::UNSPECIFIED, 4443))),
        Some(raw) => match raw.parse() {
            Ok(address) => Some(address),
            Err(_source) => {
                faults.push(Fault::Invalid {
                    knob: LISTEN_ADDR,
                    why: DETAIL_ADDRESS.to_owned(),
                });
                None
            }
        },
    };
    let chain = required(env, faults, TLS_CERT_FILE, DETAIL_CERTIFICATE);
    let key = required(env, faults, TLS_KEY_FILE, DETAIL_PRIVATE_KEY);
    Some(RunnerListenerConfig {
        address: address?,
        identity: af_stream::tls::Identity::new(chain?, key?),
    })
}

#[cfg(test)]
mod tests;
