//! GitHub's REST API, called as the App.
//!
//! A mint makes two calls under one signature, so both are written here over
//! the broker's shared `reqwest` client rather than taken from a GitHub SDK:
//! the signature is one RS256 `jsonwebtoken::encode`, and each call is one
//! request whose status this module sorts and whose body the caller's own type
//! reads. The client is the broker's, so a GitHub mint shares the connection
//! pool and timeout every other vendor's mint already uses.

use std::time::Duration;

use afd_core::clock::{after, before};
use jiff::Timestamp;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use reqwest::header::{ACCEPT, CONTENT_TYPE, USER_AGENT};
use reqwest::{RequestBuilder, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use url::Url;
use zeroize::Zeroizing;

use crate::credential::outcome::{Outcome, Retry};

/// How far an App JWT's `iat` is backdated, so a GitHub clock running behind
/// this host's does not read the token as issued in the future.
const CLOCK_DRIFT: Duration = Duration::from_mins(1);

/// How long an App JWT is good for, counted from its backdated `iat`.
///
/// GitHub refuses one whose `exp` lies more than ten minutes ahead; counted
/// from the backdated `iat`, this one lies nine ahead.
const JWT_LIFETIME: Duration = Duration::from_mins(10);

/// The media type GitHub's REST API answers in.
const MEDIA_GITHUB_JSON: &str = "application/vnd.github+json";

/// The media type a request body is sent as.
const MEDIA_JSON: &str = "application/json";

/// The header that pins the REST API version a response is shaped by.
const API_VERSION_HEADER: &str = "x-github-api-version";

/// The REST API version both calls are written against.
const API_VERSION: &str = "2022-11-28";

/// The `User-Agent` GitHub requires on every request.
const PRODUCT: &str = "agentsfleet";

/// What an App JWT claims (RFC 7519 section 4.1): who signed it and when it is good.
///
/// `iat` and `exp` are `NumericDate`, whole seconds since the epoch — the JWT's
/// own spelling of an instant.
#[derive(Debug, Serialize)]
struct Claims {
    iat: i64,
    exp: i64,
    iss: u64,
}

/// The App JWT for `app_id`, signed with `key`, good from `now`.
///
/// `None` when the key cannot sign; the caller refuses the mint for good,
/// because no retry fixes an operator's key.
pub(super) fn sign(app_id: u64, key: &EncodingKey, now: Timestamp) -> Option<Zeroizing<String>> {
    let issued = before(now, CLOCK_DRIFT);
    let claims = Claims {
        iat: issued.as_second(),
        exp: after(issued, JWT_LIFETIME).as_second(),
        iss: app_id,
    };
    jsonwebtoken::encode(&Header::new(Algorithm::RS256), &claims, key)
        .ok()
        .map(Zeroizing::new)
}

/// Why a call produced no body to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Failure {
    /// GitHub answered, with a status other than success.
    Status(StatusCode),
    /// No answer that could be read: a transport failure, a timeout, a body
    /// that dropped mid-read. The request may never have been seen.
    Unanswered,
    /// GitHub answered success with a body that is not the shape asked for.
    ///
    /// Permanent rather than retried: the same request gets the same body, so
    /// a retry spends a signature and a round trip to be refused again.
    Undecodable,
    /// The request could not be built at all, so it was never sent.
    Unsendable,
}

/// Sorts a failed call into what the caller should do about it.
///
/// A 401 or 404 is the installation being gone, which is a reconnect. Anything
/// at or above 500, and anything unanswered, is the vendor or the network,
/// which is worth retrying. Every other status, a success body that will not
/// decode, and a request that could not be built, is permanent.
impl From<Failure> for Outcome {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::Status(StatusCode::UNAUTHORIZED | StatusCode::NOT_FOUND) => {
                Self::ReconnectRequired
            }
            Failure::Status(status) if status.is_server_error() => {
                Self::MintFailed(Retry::Transient)
            }
            Failure::Unanswered => Self::MintFailed(Retry::Transient),
            Failure::Status(_) | Failure::Undecodable | Failure::Unsendable => {
                Self::MintFailed(Retry::Permanent)
            }
        }
    }
}

/// GitHub's REST API rooted at `base`, every request carrying one App JWT.
pub(super) struct AppApi<'a> {
    /// The broker's shared client.
    http: &'a reqwest::Client,
    /// The API root every path is joined onto.
    base: Url,
    /// The App JWT. Wiped when the client drops.
    bearer: Zeroizing<String>,
}

impl<'a> AppApi<'a> {
    /// The API at `base`, signed with `bearer`.
    pub(super) const fn new(
        http: &'a reqwest::Client,
        base: Url,
        bearer: Zeroizing<String>,
    ) -> Self {
        Self { http, base, bearer }
    }

    /// `GET path`, read as `T`.
    pub(super) async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, Failure> {
        self.send(self.http.get(self.url(path)?)).await
    }

    /// `POST path` with `body` as JSON, read as `T`.
    pub(super) async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, Failure> {
        let body = serde_json::to_vec(body).map_err(|_unencodable| Failure::Unsendable)?;
        let request = self
            .http
            .post(self.url(path)?)
            .header(CONTENT_TYPE, MEDIA_JSON)
            .body(body);
        self.send(request).await
    }

    fn url(&self, path: &str) -> Result<Url, Failure> {
        self.base
            .join(path)
            .map_err(|_unjoinable| Failure::Unsendable)
    }

    /// Signs and sends `request`, reading a success body as `T`.
    async fn send<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T, Failure> {
        let response = request
            .bearer_auth(self.bearer.as_str())
            .header(ACCEPT, MEDIA_GITHUB_JSON)
            .header(USER_AGENT, PRODUCT)
            .header(API_VERSION_HEADER, API_VERSION)
            .send()
            .await
            .map_err(|_transport| Failure::Unanswered)?;
        let status = response.status();
        if !status.is_success() {
            return Err(Failure::Status(status));
        }
        let body = response
            .bytes()
            .await
            .map_err(|_dropped| Failure::Unanswered)?;
        serde_json::from_slice(&body).map_err(|_undecodable| Failure::Undecodable)
    }
}
