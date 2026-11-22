//! Transport metadata only; the common HTTP middleware authenticates the runner.

use af_core::id::Uuid7;
use af_wire::protocol::{self, StreamKind};
use http::HeaderMap;

use crate::{Result, error::ErrorKind};

const AUTHORIZATION_HEADER: &str = "authorization";

/// Every secondary stream carries a current owner; only control may carry a bearer.
#[derive(Debug)]
pub(crate) enum StreamHeaders {
    Control { previous: Option<Uuid7> },
    Work { connection: Uuid7 },
    Reports { connection: Uuid7 },
    Progress { connection: Uuid7 },
}

impl TryFrom<&HeaderMap> for StreamHeaders {
    type Error = crate::Error;

    fn try_from(headers: &HeaderMap) -> Result<Self> {
        version(headers)?;
        let kind = StreamKind::parse(required(headers, protocol::STREAM_KIND_HEADER)?)
            .ok_or(ErrorKind::UnknownStream)?;
        let connection = one(headers, protocol::CONNECTION_HEADER)?
            .map(|raw| {
                Uuid7::parse(raw).map_err(|source| ErrorKind::ConnectionIdentifier { source })
            })
            .transpose()?;
        let bearer = one(headers, AUTHORIZATION_HEADER)?.is_some();
        match kind {
            StreamKind::Control => {
                if !bearer {
                    return Err(ErrorKind::ControlBearerMissing.into());
                }
                Ok(Self::Control {
                    previous: connection,
                })
            }
            StreamKind::Work => {
                secondary(bearer, connection).map(|connection| Self::Work { connection })
            }
            StreamKind::Reports => {
                secondary(bearer, connection).map(|connection| Self::Reports { connection })
            }
            StreamKind::Progress => {
                secondary(bearer, connection).map(|connection| Self::Progress { connection })
            }
        }
    }
}

/// The protocol revision a stream's request names, refused unless it is the
/// one this daemon speaks, [`protocol::VERSION`]. Every stream names one.
pub(crate) fn version(headers: &HeaderMap) -> Result<()> {
    if required(headers, protocol::VERSION_HEADER)? != protocol::VERSION {
        return Err(ErrorKind::UnsupportedVersion.into());
    }
    Ok(())
}

/// A secondary lane binds to the authenticated connection and never carries its own bearer.
fn secondary(bearer: bool, connection: Option<Uuid7>) -> Result<Uuid7> {
    if bearer {
        return Err(ErrorKind::UnexpectedBearer.into());
    }
    Ok(connection.ok_or(ErrorKind::MissingHeader {
        header: protocol::CONNECTION_HEADER,
    })?)
}

impl StreamHeaders {
    pub(crate) const fn stream_kind(&self) -> StreamKind {
        match self {
            Self::Control { .. } => StreamKind::Control,
            Self::Work { .. } => StreamKind::Work,
            Self::Reports { .. } => StreamKind::Reports,
            Self::Progress { .. } => StreamKind::Progress,
        }
    }

    pub(crate) fn connection(&self) -> Option<&Uuid7> {
        match self {
            Self::Control { previous } => previous.as_ref(),
            Self::Work { connection }
            | Self::Reports { connection }
            | Self::Progress { connection } => Some(connection),
        }
    }
}

fn required<'a>(headers: &'a HeaderMap, name: &'static str) -> Result<&'a str> {
    one(headers, name)?.ok_or_else(|| ErrorKind::MissingHeader { header: name }.into())
}

fn one<'a>(headers: &'a HeaderMap, name: &'static str) -> Result<Option<&'a str>> {
    let mut values = headers.get_all(name).iter();
    let first = values
        .next()
        .map(|value| {
            value.to_str().map_err(|source| ErrorKind::HeaderEncoding {
                header: name,
                source,
            })
        })
        .transpose()?;
    if values.next().is_some() {
        return Err(ErrorKind::RepeatedHeader { header: name }.into());
    }
    Ok(first)
}

#[cfg(test)]
mod tests {
    use super::{StreamHeaders, StreamKind};
    use crate::error::ErrorKind;
    use af_wire::protocol;
    use http::{HeaderMap, HeaderValue};

    type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

    const SESSION: &str = "019f0000-0000-7000-8000-000000000001";

    fn headers(
        pairs: &[(&'static str, &'static [u8])],
    ) -> Result<HeaderMap, http::header::InvalidHeaderValue> {
        let mut map = HeaderMap::new();
        map.append(
            protocol::VERSION_HEADER,
            HeaderValue::from_static(protocol::VERSION),
        );
        for (name, value) in pairs {
            map.append(*name, HeaderValue::from_bytes(value)?);
        }
        Ok(map)
    }

    #[test]
    fn control_carries_the_bearer_and_secondary_lanes_bind_the_connection() -> TestResult {
        let control = StreamHeaders::try_from(&headers(&[
            (protocol::STREAM_KIND_HEADER, b"control"),
            ("authorization", b"Bearer agt_r_fixture"),
            (protocol::CONNECTION_HEADER, SESSION.as_bytes()),
        ])?)?;
        assert_eq!(control.stream_kind(), StreamKind::Control);
        assert_eq!(
            control.connection().map(af_core::id::Uuid7::as_str),
            Some(SESSION),
            "a reconnecting control names the connection it replaces"
        );
        let fresh = StreamHeaders::try_from(&headers(&[
            (protocol::STREAM_KIND_HEADER, b"control"),
            ("authorization", b"Bearer agt_r_fixture"),
        ])?)?;
        assert!(fresh.connection().is_none(), "{fresh:?}");
        for (kind, name) in [
            (StreamKind::Work, b"work".as_slice()),
            (StreamKind::Reports, b"reports"),
            (StreamKind::Progress, b"progress"),
        ] {
            let mut map = headers(&[(protocol::CONNECTION_HEADER, SESSION.as_bytes())])?;
            map.append(protocol::STREAM_KIND_HEADER, HeaderValue::from_bytes(name)?);
            let bound = StreamHeaders::try_from(&map)?;
            assert_eq!(bound.stream_kind(), kind);
            assert_eq!(
                bound.connection().map(af_core::id::Uuid7::as_str),
                Some(SESSION)
            );
        }
        Ok(())
    }

    type Pairs<'a> = &'a [(&'static str, &'static [u8])];
    type Expected = fn(&ErrorKind) -> bool;

    #[test]
    fn every_lane_binding_violation_is_refused_with_its_own_kind() -> TestResult {
        let cases: [(Pairs<'_>, Expected); 7] = [
            (&[(protocol::STREAM_KIND_HEADER, b"control")], |kind| {
                matches!(kind, ErrorKind::ControlBearerMissing)
            }),
            (
                &[
                    (protocol::STREAM_KIND_HEADER, b"work"),
                    (protocol::CONNECTION_HEADER, SESSION.as_bytes()),
                    ("authorization", b"Bearer agt_r_fixture"),
                ],
                |kind| matches!(kind, ErrorKind::UnexpectedBearer),
            ),
            (
                &[(protocol::STREAM_KIND_HEADER, b"reports")],
                |kind| matches!(kind, ErrorKind::MissingHeader { header } if *header == protocol::CONNECTION_HEADER),
            ),
            (
                &[
                    (protocol::STREAM_KIND_HEADER, b"progress"),
                    (protocol::CONNECTION_HEADER, SESSION.as_bytes()),
                    (protocol::CONNECTION_HEADER, SESSION.as_bytes()),
                ],
                |kind| matches!(kind, ErrorKind::RepeatedHeader { header } if *header == protocol::CONNECTION_HEADER),
            ),
            (
                &[
                    (protocol::STREAM_KIND_HEADER, b"work"),
                    (protocol::CONNECTION_HEADER, b"\xff"),
                ],
                |kind| matches!(kind, ErrorKind::HeaderEncoding { header, .. } if *header == protocol::CONNECTION_HEADER),
            ),
            (
                &[
                    (protocol::STREAM_KIND_HEADER, b"work"),
                    (protocol::CONNECTION_HEADER, b"not-a-connection"),
                ],
                |kind| matches!(kind, ErrorKind::ConnectionIdentifier { .. }),
            ),
            (&[(protocol::STREAM_KIND_HEADER, b"telemetry")], |kind| {
                matches!(kind, ErrorKind::UnknownStream)
            }),
        ];
        for (pairs, expected) in cases {
            let refused = StreamHeaders::try_from(&headers(pairs)?).err();
            assert!(
                refused.as_ref().is_some_and(|error| expected(error.kind())),
                "{refused:?}"
            );
        }
        Ok(())
    }
}
