//! The UDP datagram ceiling both QUIC endpoints send under.
//!
//! Fixed at this value for now: a later milestone makes it configurable through the TOML
//! configuration. Until then this constant is the whole sizing policy.
//!
//! An IPv6 link must carry 1280 bytes, so a datagram that fits in 1280 minus the headers crosses
//! any conforming path. The daemon caps what it sends at this size. The runner advertises it as
//! the largest datagram it accepts, which also holds back a daemon that has no ceiling of its own
//! on the connection's first path. The runner's own datagrams stay at quiche's 1200-byte default.

/// The smallest MTU an IPv6 link must carry.
const IPV6_MIN_MTU: usize = 1280;
/// The IPv6 header (40 bytes) and the UDP header (8) that ride on every datagram.
const IPV6_UDP_HEADER_BYTES: usize = 48;
/// The smallest UDP payload limit a QUIC endpoint may advertise; a peer refuses a lower one.
const QUIC_MIN_DATAGRAM_BYTES: usize = 1200;

/// Largest UDP payload either QUIC endpoint puts on the wire.
pub(crate) const MAX_DATAGRAM_BYTES: usize = IPV6_MIN_MTU - IPV6_UDP_HEADER_BYTES;

const _: () = assert!(
    MAX_DATAGRAM_BYTES >= QUIC_MIN_DATAGRAM_BYTES,
    "a datagram ceiling under the QUIC minimum makes the endpoint advertise an invalid limit"
);
