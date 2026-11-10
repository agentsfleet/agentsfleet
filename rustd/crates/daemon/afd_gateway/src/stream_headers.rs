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
    Work { session: Uuid7 },
    Reports { session: Uuid7 },
    Activity { session: Uuid7 },
}

impl TryFrom<&HeaderMap> for StreamHeaders {
    type Error = crate::Error;

    fn try_from(headers: &HeaderMap) -> Result<Self> {
        let version = required(headers, protocol::VERSION_HEADER)?;
        if version != protocol::VERSION {
            return Err(ErrorKind::UnsupportedVersion.into());
        }
        let kind = StreamKind::parse(required(headers, protocol::STREAM_KIND_HEADER)?)
            .ok_or(ErrorKind::UnknownStream)?;
        let session = one(headers, protocol::SESSION_HEADER)?
            .map(|raw| Uuid7::parse(raw).map_err(|source| ErrorKind::SessionIdentifier { source }))
            .transpose()?;
        let bearer = one(headers, AUTHORIZATION_HEADER)?.is_some();
        match kind {
            StreamKind::Control => {
                if !bearer {
                    return Err(ErrorKind::ControlBearerMissing.into());
                }
                Ok(Self::Control { previous: session })
            }
            StreamKind::Work => secondary(bearer, session).map(|session| Self::Work { session }),
            StreamKind::Reports => {
                secondary(bearer, session).map(|session| Self::Reports { session })
            }
            StreamKind::Activity => {
                secondary(bearer, session).map(|session| Self::Activity { session })
            }
        }
    }
}

/// A secondary lane binds to the authenticated session and never carries its own bearer.
fn secondary(bearer: bool, session: Option<Uuid7>) -> Result<Uuid7> {
    if bearer {
        return Err(ErrorKind::UnexpectedBearer.into());
    }
    Ok(session.ok_or(ErrorKind::MissingHeader {
        header: protocol::SESSION_HEADER,
    })?)
}

impl StreamHeaders {
    pub(crate) const fn stream_kind(&self) -> StreamKind {
        match self {
            Self::Control { .. } => StreamKind::Control,
            Self::Work { .. } => StreamKind::Work,
            Self::Reports { .. } => StreamKind::Reports,
            Self::Activity { .. } => StreamKind::Activity,
        }
    }

    pub(crate) fn session(&self) -> Option<&Uuid7> {
        match self {
            Self::Control { previous } => previous.as_ref(),
            Self::Work { session } | Self::Reports { session } | Self::Activity { session } => {
                Some(session)
            }
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

    const SESSION: &str = "019f0000-0000-7000-8000-000000000001";

    fn headers(pairs: &[(&'static str, &'static [u8])]) -> HeaderMap {
        let mut map = HeaderMap::new();
        map.append(
            protocol::VERSION_HEADER,
            HeaderValue::from_static(protocol::VERSION),
        );
        for (name, value) in pairs {
            if let Ok(value) = HeaderValue::from_bytes(value) {
                map.append(*name, value);
            }
        }
        map
    }

    #[test]
    fn control_carries_the_bearer_and_secondary_lanes_bind_the_session() {
        let control = StreamHeaders::try_from(&headers(&[
            (protocol::STREAM_KIND_HEADER, b"control"),
            ("authorization", b"Bearer agt_r_fixture"),
            (protocol::SESSION_HEADER, SESSION.as_bytes()),
        ]));
        let control = control.map_err(|error| error.to_string());
        assert!(
            matches!(&control, Ok(StreamHeaders::Control { previous: Some(_) })),
            "{control:?}"
        );
        for (kind, name) in [
            (StreamKind::Work, b"work".as_slice()),
            (StreamKind::Reports, b"reports"),
            (StreamKind::Activity, b"activity"),
        ] {
            let mut map = headers(&[(protocol::SESSION_HEADER, SESSION.as_bytes())]);
            map.append(
                protocol::STREAM_KIND_HEADER,
                HeaderValue::from_bytes(name).unwrap_or(HeaderValue::from_static("")),
            );
            let bound = StreamHeaders::try_from(&map).map_err(|error| error.to_string());
            assert_eq!(
                bound.as_ref().map(StreamHeaders::stream_kind).ok(),
                Some(kind)
            );
            assert_eq!(
                bound
                    .as_ref()
                    .ok()
                    .and_then(StreamHeaders::session)
                    .map(af_core::id::Uuid7::as_str),
                Some(SESSION)
            );
        }
    }

    type Pairs<'a> = &'a [(&'static str, &'static [u8])];
    type Expected = fn(&ErrorKind) -> bool;

    #[test]
    fn every_lane_binding_violation_is_refused_with_its_own_kind() {
        let cases: [(Pairs<'_>, Expected); 5] = [
            (&[(protocol::STREAM_KIND_HEADER, b"control")], |kind| {
                matches!(kind, ErrorKind::ControlBearerMissing)
            }),
            (
                &[
                    (protocol::STREAM_KIND_HEADER, b"work"),
                    (protocol::SESSION_HEADER, SESSION.as_bytes()),
                    ("authorization", b"Bearer agt_r_fixture"),
                ],
                |kind| matches!(kind, ErrorKind::UnexpectedBearer),
            ),
            (
                &[(protocol::STREAM_KIND_HEADER, b"reports")],
                |kind| matches!(kind, ErrorKind::MissingHeader { header } if *header == protocol::SESSION_HEADER),
            ),
            (
                &[
                    (protocol::STREAM_KIND_HEADER, b"activity"),
                    (protocol::SESSION_HEADER, SESSION.as_bytes()),
                    (protocol::SESSION_HEADER, SESSION.as_bytes()),
                ],
                |kind| matches!(kind, ErrorKind::RepeatedHeader { header } if *header == protocol::SESSION_HEADER),
            ),
            (
                &[
                    (protocol::STREAM_KIND_HEADER, b"work"),
                    (protocol::SESSION_HEADER, b"\xff"),
                ],
                |kind| matches!(kind, ErrorKind::HeaderEncoding { header, .. } if *header == protocol::SESSION_HEADER),
            ),
        ];
        for (pairs, expected) in cases {
            let refused = StreamHeaders::try_from(&headers(pairs));
            assert!(
                refused.as_ref().is_err_and(|error| expected(error.kind())),
                "{:?}",
                refused.map_err(|error| error.to_string())
            );
        }
    }
}
