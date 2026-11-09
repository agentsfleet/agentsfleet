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
        let bearer = one(headers, AUTHORIZATION_HEADER)?;
        match kind {
            StreamKind::Control => {
                if bearer.is_none() {
                    return Err(ErrorKind::ControlBearerMissing.into());
                }
                Ok(Self::Control { previous: session })
            }
            kind => {
                if bearer.is_some() {
                    return Err(ErrorKind::UnexpectedBearer.into());
                }
                let session = session.ok_or(ErrorKind::MissingHeader {
                    header: protocol::SESSION_HEADER,
                })?;
                Ok(match kind {
                    StreamKind::Work => Self::Work { session },
                    StreamKind::Reports => Self::Reports { session },
                    StreamKind::Activity => Self::Activity { session },
                    StreamKind::Control => return Err(ErrorKind::UnknownStream.into()),
                })
            }
        }
    }
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
