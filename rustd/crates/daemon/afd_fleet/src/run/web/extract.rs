//! A vendor outage can authorize a single direct read only after checking the
//! live run and public fleet again. A missing or ambiguous reply cannot.

use af_core::id::Uuid7;
use af_egress::{Resolve as _, SystemResolver};
use af_providers::web::WebFailure;
use af_wire::web::{ExtractRequest, ExtractResponse};
use garde::Valid;
use reqwest::Url;
use std::time::Duration;
use tokio::time::{Instant, timeout_at};

use super::{Claim, EXTRACT, Failure, RunService, VendorName, remaining};
use crate::Result;

impl RunService {
    /// Read one public HTTPS page through Parallel's full-content API.
    ///
    /// # Errors
    /// Refuses a blocked destination, stale fence, restricted fleet, missing
    /// key, reservation failure, invalid response, outage or elapsed deadline.
    pub async fn web_extract(
        &self,
        runner: &Uuid7,
        run: Uuid7,
        request: &Valid<ExtractRequest>,
        deadline: Instant,
    ) -> Result<ExtractResponse> {
        let claim = Claim {
            run: &run,
            runner,
            fence: request.fencing_token,
        };
        timeout_at(deadline, self.extract_page(&claim, &request.url, deadline))
            .await
            .map_err(|_| Failure::Deadline)?
    }

    async fn extract_page(
        &self,
        claim: &Claim<'_>,
        url: &str,
        deadline: Instant,
    ) -> Result<ExtractResponse> {
        claim.authorize(&self.runs, None, deadline).await?;
        public_page(url).await?;
        let name = VendorName::Parallel;
        let key = self.web_key(name).await?.ok_or(Failure::Unconfigured)?;
        if let Some(wait) = self.runs.web.wait(name).await {
            return self.extract_outage(claim, Some(wait), deadline).await;
        }
        let clients = self.runs.web.clients().await?;
        let fleet = claim
            .authorize(&self.runs, Some((name, EXTRACT)), deadline)
            .await?;
        let budget = remaining(deadline)?;
        let start = Instant::now();
        let result = clients.parallel.extract(&key, url, budget).await;
        self.web_observed(claim, &fleet, name, EXTRACT, start, &result)
            .await;
        match result {
            Ok(response) => Ok(response),
            Err(WebFailure::Transport) => self.extract_outage(claim, None, deadline).await,
            Err(WebFailure::Status {
                status,
                retry_after,
            }) if temporary(status) => self.extract_outage(claim, retry_after, deadline).await,
            Err(WebFailure::Page { status }) if temporary(status) => {
                self.extract_outage(claim, None, deadline).await
            }
            Err(WebFailure::Page { status }) => Err(Failure::Page(status).into()),
            Err(WebFailure::InvalidResponse) => Err(Failure::InvalidResponse.into()),
            Err(WebFailure::InvalidCredential | WebFailure::Status { .. }) => {
                Err(Failure::Unconfigured.into())
            }
        }
    }

    async fn extract_outage(
        &self,
        claim: &Claim<'_>,
        wait: Option<Duration>,
        deadline: Instant,
    ) -> Result<ExtractResponse> {
        remaining(deadline)?;
        claim.authorize(&self.runs, None, deadline).await?;
        remaining(deadline)?;
        Err(Failure::ExtractUnavailable(wait).into())
    }
}

const fn temporary(status: u16) -> bool {
    status == 429 || (status >= 500 && status < 600)
}

async fn public_page(input: &str) -> Result<()> {
    let url = Url::parse(input).map_err(|_| Failure::InvalidUrl)?;
    let host = url.host_str().ok_or(Failure::InvalidUrl)?;
    // WHATWG serialization discards an empty userinfo delimiter. The vendor
    // receives the original spelling, so check its authority as well.
    let empty_userinfo = input.parse::<http::Uri>().is_ok_and(|uri| {
        uri.authority()
            .is_some_and(|authority| authority.as_str().contains('@'))
    });
    if url.scheme() != "https"
        || empty_userinfo
        || !url.username().is_empty()
        || url.password().is_some()
        || af_egress::blocked_literal(&url)
    {
        return Err(Failure::InvalidUrl.into());
    }
    if url.domain().is_some() {
        let addresses = SystemResolver
            .resolve(host)
            .await
            .map_err(|_| Failure::InvalidUrl)?;
        if !af_egress::unblocked(addresses).is_ok_and(|addrs| !addrs.is_empty()) {
            return Err(Failure::InvalidUrl.into());
        }
    }
    Ok(())
}
