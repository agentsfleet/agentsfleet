//! Fixed-origin requests, bounded streaming bodies, and no physical retries.

use afd_credential::credential::outcome::Minted;
use futures_util::StreamExt as _;
use reqwest::{Method, StatusCode};
use serde::{Serialize, de::DeserializeOwned};
use crate::{Result, error::ErrorKind, task::TaskRefusal};

pub(in crate::task) struct Session<'a> {
    client: &'a reqwest::Client, credential: Minted, pub(super) installation_id: u64,
    read_only: bool,
    deadline: jiff::Timestamp,
}

impl<'a> Session<'a> {
    pub(super) const fn new(client: &'a reqwest::Client, credential: Minted, installation_id: u64, read_only: bool) -> Self {
        let deadline = credential.expires_at;
        Self { client, credential, installation_id, read_only, deadline }
    }

    pub(in crate::task) fn with_deadline(mut self, deadline: jiff::Timestamp) -> Self {
        self.deadline = self.deadline.min(deadline);
        self
    }

    pub(super) fn read_credential(&self) -> Result<&Minted> {
        if !self.read_only { return Err(TaskRefusal::InvalidTarget.into()); }
        Ok(&self.credential)
    }

    pub(super) async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.json(Method::GET, path, None::<&()>).await
    }

    pub(super) async fn post<T: DeserializeOwned>(&self, path: &str, body: &impl Serialize) -> Result<T> {
        self.json(Method::POST, path, Some(body)).await
    }

    async fn json<T: DeserializeOwned>(&self, method: Method, path: &str, body: Option<&impl Serialize>) -> Result<T> {
        let bytes = self.bytes(method, path, body).await?;
        serde_json::from_slice(&bytes).map_err(afd_core::json::Unreadable::from).map_err(Into::into)
    }

    pub(super) async fn bytes(&self, method: Method, path: &str, body: Option<&impl Serialize>) -> Result<Vec<u8>> {
        let mut url = reqwest::Url::parse("https://api.github.com/").map_err(|_invalid| TaskRefusal::InvalidTarget)?;
        // No caller-supplied authority or query can redirect a credential.
        let (path, query) = path.split_once('?').map_or((path, None), |(path, query)| (path, Some(query)));
        url.set_path(path);
        url.set_query(query);
        let mut request = self.client.request(method, url)
            .bearer_auth(self.credential.token.as_str()).header("user-agent", "agentsfleet")
            .header("x-github-api-version", "2022-11-28").header("accept", "application/vnd.github+json");
        if let Some(body) = body {
            request = request.header("content-type", "application/json")
                .body(serde_json::to_vec(body).map_err(afd_core::json::Unreadable::from)?);
        }
        let remaining = (self.deadline - afd_core::clock::now()).as_secs_f64().min(10.0);
        if remaining <= 0.0 { return Err(TaskRefusal::StaleState.into()); }
        let response = request.timeout(std::time::Duration::from_secs_f64(remaining)).send().await?;
        let status = response.status();
        if status != StatusCode::OK && status != StatusCode::CREATED {
            if status == StatusCode::TOO_MANY_REQUESTS || status == StatusCode::FORBIDDEN
                && response.headers().get("x-ratelimit-remaining").is_some_and(|value| value == "0") {
                let headers = response.headers();
                let now = afd_core::clock::now();
                let retry = headers.get("retry-after").and_then(|value| value.to_str().ok()).and_then(|value| value.parse::<u64>().ok())
                    .and_then(|seconds| now.checked_add(jiff::SignedDuration::from_secs(i64::try_from(seconds).ok()?)).ok());
                let reset = headers.get("x-ratelimit-reset").and_then(|value| value.to_str().ok()).and_then(|value| value.parse().ok())
                    .and_then(|seconds| jiff::Timestamp::from_second(seconds).ok());
                let until = retry.into_iter().chain(reset).max().filter(|until| *until > now)
                    .unwrap_or_else(|| afd_core::clock::after(now, std::time::Duration::from_secs(300)));
                return Err(ErrorKind::TaskRemoteCooldown { status: status.as_u16(), until }.into());
            }
            return Err(ErrorKind::TaskRemoteStatus { status: status.as_u16() }.into());
        }
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if bytes.len().saturating_add(chunk.len()) > 2 * 1024 * 1024 { return Err(TaskRefusal::InvalidTarget.into()); }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
}

pub(super) fn path(parts: &[&str]) -> Result<String> {
    let mut url = reqwest::Url::parse("https://api.github.com/").map_err(|_invalid| TaskRefusal::InvalidTarget)?;
    url.path_segments_mut().map_err(|_invalid| TaskRefusal::InvalidTarget)?.extend(parts);
    Ok(url.path().to_owned())
}
