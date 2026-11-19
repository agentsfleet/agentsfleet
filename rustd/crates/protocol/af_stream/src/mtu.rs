//! The UDP datagram ceiling both QUIC endpoints send under.
//!
//! The default fits the 1280-byte IPv6 minimum after the IP and UDP headers.
//! The daemon can select a validated UDP payload ceiling through its application file.
//! The peer's advertised receive limit also constrains every connection.

use garde::Validate;

/// The smallest MTU an IPv6 link must carry.
const IPV6_MIN_MTU: usize = 1280;
/// The IPv6 header (40 bytes) and the UDP header (8) that ride on every datagram.
const IPV6_UDP_HEADER_BYTES: usize = 48;
/// The smallest UDP payload limit a QUIC endpoint may advertise; a peer refuses a lower one.
const QUIC_MIN_DATAGRAM_BYTES: usize = 1200;

/// Default daemon send ceiling and runner receive advertisement.
pub(crate) const MAX_DATAGRAM_BYTES: usize = IPV6_MIN_MTU - IPV6_UDP_HEADER_BYTES;

const _: () = assert!(
    MAX_DATAGRAM_BYTES >= QUIC_MIN_DATAGRAM_BYTES,
    "a datagram ceiling under the QUIC minimum makes the endpoint advertise an invalid limit"
);

/// A validated ceiling on the UDP payload the daemon sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatagramLimit(usize);

#[derive(Validate)]
struct DatagramBytes {
    #[garde(range(min = DatagramLimit::MIN_BYTES, max = DatagramLimit::MAX_BYTES))]
    bytes: usize,
}

impl DatagramLimit {
    /// QUIC requires support for UDP payloads of at least 1200 bytes.
    pub const MIN_BYTES: usize = QUIC_MIN_DATAGRAM_BYTES;
    /// Largest IPv4 UDP payload after minimum IP and UDP headers.
    pub const MAX_BYTES: usize = 65_507;

    /// UDP payload ceiling in bytes, excluding IP and UDP headers.
    #[must_use]
    pub const fn bytes(self) -> usize {
        self.0
    }
}

impl Default for DatagramLimit {
    fn default() -> Self {
        Self(MAX_DATAGRAM_BYTES)
    }
}

impl TryFrom<usize> for DatagramLimit {
    type Error = crate::Error;

    fn try_from(bytes: usize) -> crate::Result<Self> {
        let value = garde::Unvalidated::new(DatagramBytes { bytes }).validate()?;
        Ok(Self(value.bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::DatagramLimit;

    #[test]
    fn a_default_datagram_fits_the_minimum_ipv6_path() {
        assert_eq!(DatagramLimit::default().bytes(), 1232);
    }

    #[test]
    fn a_payload_ceiling_must_fit_udp_and_support_a_quic_initial() -> crate::Result<()> {
        for bytes in [0, 1199, 65_508, usize::MAX] {
            let failure = DatagramLimit::try_from(bytes)
                .err()
                .ok_or(crate::error::ErrorKind::Protocol)?;
            assert_eq!(
                failure.code(),
                af_core::error_code::INTERNAL_OPERATION_FAILED
            );
            let bounds = std::error::Error::source(&failure)
                .and_then(|source| source.downcast_ref::<garde::Report>())
                .ok_or(crate::error::ErrorKind::Protocol)?;
            assert!(bounds.iter().any(|(path, _)| path.to_string() == "bytes"));
        }
        for bytes in [1200, 1232, 65_507] {
            assert_eq!(DatagramLimit::try_from(bytes)?.bytes(), bytes);
        }
        Ok(())
    }
}
