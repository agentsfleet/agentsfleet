//! Checked HTTP heads; request handling receives complete method, URI and status types.

use std::collections::BTreeMap;

use http::{
    HeaderMap, Request, Response,
    header::{AUTHORIZATION, HeaderName, HeaderValue},
};
use tokio_quiche::quiche::h3::{Header, NameValue as _};

use crate::{Incoming, Result, error::ErrorKind};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Pseudo {
    Method,
    Path,
    Authority,
    Scheme,
    Status,
}

impl TryFrom<&[u8]> for Pseudo {
    type Error = crate::Error;

    fn try_from(name: &[u8]) -> Result<Self> {
        match name {
            b":method" => Ok(Self::Method),
            b":path" => Ok(Self::Path),
            b":authority" => Ok(Self::Authority),
            b":scheme" => Ok(Self::Scheme),
            b":status" => Ok(Self::Status),
            _ => Err(ErrorKind::Protocol.into()),
        }
    }
}

#[derive(Default)]
struct Fields<'a> {
    pseudo: BTreeMap<Pseudo, &'a [u8]>,
    regular: HeaderMap,
}

impl<'a> TryFrom<&'a [Header]> for Fields<'a> {
    type Error = crate::Error;

    fn try_from(headers: &'a [Header]) -> Result<Self> {
        headers
            .iter()
            .try_fold(Self::default(), |mut fields, header| {
                if header.name().starts_with(b":") {
                    let name = Pseudo::try_from(header.name())?;
                    if !fields.regular.is_empty()
                        || fields.pseudo.insert(name, header.value()).is_some()
                    {
                        return Err(ErrorKind::Protocol.into());
                    }
                } else {
                    let name = HeaderName::from_bytes(header.name())?;
                    let mut value = HeaderValue::from_bytes(header.value())?;
                    value.set_sensitive(name == AUTHORIZATION);
                    fields.regular.append(name, value);
                }
                Ok(fields)
            })
    }
}

impl Fields<'_> {
    fn take(&mut self, name: Pseudo) -> Result<&[u8]> {
        self.pseudo
            .remove(&name)
            .ok_or_else(|| ErrorKind::Protocol.into())
    }

    fn finish(self) -> Result<HeaderMap> {
        if !self.pseudo.is_empty() {
            return Err(ErrorKind::Protocol.into());
        }
        Ok(self.regular)
    }
}

pub(super) struct RequestHead(http::request::Parts);

impl TryFrom<&[Header]> for RequestHead {
    type Error = crate::Error;

    fn try_from(headers: &[Header]) -> Result<Self> {
        let mut fields = Fields::try_from(headers)?;
        let method = http::Method::from_bytes(fields.take(Pseudo::Method)?)?;
        if fields.take(Pseudo::Scheme)? != b"https" {
            return Err(ErrorKind::Protocol.into());
        }
        let uri = http::Uri::builder()
            .scheme("https")
            .authority(fields.take(Pseudo::Authority)?)
            .path_and_query(fields.take(Pseudo::Path)?)
            .build()?;
        let (mut parts, ()) = Request::builder()
            .version(http::Version::HTTP_3)
            .method(method)
            .uri(uri)
            .body(())?
            .into_parts();
        parts.headers = fields.finish()?;
        Ok(Self(parts))
    }
}

impl RequestHead {
    pub(super) fn attach(self, body: Incoming) -> Request<Incoming> {
        Request::from_parts(self.0, body)
    }
}

pub(super) struct ResponseHead(http::response::Parts);

impl TryFrom<&[Header]> for ResponseHead {
    type Error = crate::Error;

    fn try_from(headers: &[Header]) -> Result<Self> {
        let mut fields = Fields::try_from(headers)?;
        let status = http::StatusCode::from_bytes(fields.take(Pseudo::Status)?)?;
        let (mut parts, ()) = Response::builder()
            .version(http::Version::HTTP_3)
            .status(status)
            .body(())?
            .into_parts();
        parts.headers = fields.finish()?;
        Ok(Self(parts))
    }
}

impl ResponseHead {
    pub(super) fn attach(self, body: Incoming) -> Response<Incoming> {
        Response::from_parts(self.0, body)
    }
}

pub(super) struct Encoded(Vec<Header>);

impl TryFrom<http::request::Parts> for Encoded {
    type Error = crate::Error;

    fn try_from(parts: http::request::Parts) -> Result<Self> {
        if parts.uri.scheme_str() != Some("https") {
            return Err(ErrorKind::Protocol.into());
        }
        let authority = parts.uri.authority().ok_or(ErrorKind::Protocol)?;
        let path = parts.uri.path_and_query().ok_or(ErrorKind::Protocol)?;
        let mut headers = vec![
            Header::new(b":method", parts.method.as_str().as_bytes()),
            Header::new(b":scheme", b"https"),
            Header::new(b":authority", authority.as_str().as_bytes()),
            Header::new(b":path", path.as_str().as_bytes()),
        ];
        headers.extend(
            parts
                .headers
                .iter()
                .map(|(name, value)| Header::new(name.as_str().as_bytes(), value.as_bytes())),
        );
        Ok(Self(headers))
    }
}

impl From<http::response::Parts> for Encoded {
    fn from(parts: http::response::Parts) -> Self {
        let mut headers = vec![Header::new(b":status", parts.status.as_str().as_bytes())];
        headers.extend(
            parts
                .headers
                .iter()
                .map(|(name, value)| Header::new(name.as_str().as_bytes(), value.as_bytes())),
        );
        Self(headers)
    }
}

impl From<Encoded> for Vec<Header> {
    fn from(head: Encoded) -> Self {
        head.0
    }
}

#[cfg(test)]
mod tests {
    use super::{Encoded, RequestHead, ResponseHead};
    use tokio_quiche::quiche::h3::Header;

    fn request() -> Vec<Header> {
        vec![
            Header::new(b":method", b"POST"),
            Header::new(b":scheme", b"https"),
            Header::new(b":authority", b"localhost"),
            Header::new(b":path", b"/v1/runners/me/stream"),
        ]
    }

    #[test]
    fn a_checked_request_has_complete_http_types_and_redacts_authorization() -> crate::Result<()> {
        let mut headers = request();
        headers.push(Header::new(b"authorization", b"fixture"));
        let head = RequestHead::try_from(headers.as_slice())?;
        assert_eq!(head.0.method, http::Method::POST);
        assert_eq!(head.0.uri, "https://localhost/v1/runners/me/stream");
        assert_eq!(
            head.0
                .headers
                .get(http::header::AUTHORIZATION)
                .map(http::HeaderValue::is_sensitive),
            Some(true)
        );
        Ok(())
    }

    #[test]
    fn duplicated_unknown_or_misplaced_pseudo_headers_are_refused() {
        for extra in [
            Header::new(b":method", b"GET"),
            Header::new(b":other", b"x"),
            Header::new(b":status", b"200"),
        ] {
            let mut headers = request();
            headers.push(extra);
            assert!(RequestHead::try_from(headers.as_slice()).is_err());
        }
        let mut headers = vec![Header::new(b"content-type", b"application/json")];
        headers.extend(request());
        assert!(RequestHead::try_from(headers.as_slice()).is_err());
    }

    #[test]
    fn missing_request_fields_and_invalid_response_status_are_refused() {
        assert!(RequestHead::try_from([].as_slice()).is_err());
        assert!(ResponseHead::try_from([Header::new(b":status", b"invalid")].as_slice()).is_err());
        assert!(ResponseHead::try_from(request().as_slice()).is_err());
    }

    #[test]
    fn typed_http_heads_round_trip_through_the_native_encoding() -> crate::Result<()> {
        let head = RequestHead::try_from(request().as_slice())?;
        let expected = head.0.uri.clone();
        let encoded: Vec<Header> = Encoded::try_from(head.0)?.into();
        let restored = RequestHead::try_from(encoded.as_slice())?;
        assert_eq!(restored.0.uri, expected);
        let (parts, ()) = http::Response::builder()
            .status(http::StatusCode::FORBIDDEN)
            .body(())?
            .into_parts();
        let encoded: Vec<Header> = Encoded::from(parts).into();
        assert_eq!(
            ResponseHead::try_from(encoded.as_slice())?.0.status,
            http::StatusCode::FORBIDDEN
        );
        Ok(())
    }
}
