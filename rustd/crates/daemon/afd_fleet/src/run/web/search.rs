//! Fixed vendor order, one shared deadline and at most one physical try each.

use af_core::id::Uuid7;
use af_wire::web::{SearchRequest, SearchResponse};
use garde::Valid;
use std::time::Duration;
use tokio::time::{Instant, timeout_at};

use super::{Claim, Failure, RunService, SEARCH, VendorName, WebFailure, remaining, status};
use crate::Result;

impl RunService {
    /// Search under the run's current public egress policy.
    ///
    /// # Errors
    /// Refuses expired or private runs, keyless vendors, uncommitted
    /// reservations and a search that reaches its deadline or exhausts vendors.
    pub async fn web_search(
        &self,
        runner: &Uuid7,
        run: Uuid7,
        request: &Valid<SearchRequest>,
        deadline: Instant,
    ) -> Result<SearchResponse> {
        let claim = Claim {
            run: &run,
            runner,
            fence: request.fencing_token,
        };
        timeout_at(
            deadline,
            self.search_vendors(&claim, &request.query, deadline),
        )
        .await
        .map_err(|_| Failure::Deadline)?
    }

    async fn search_vendors(
        &self,
        claim: &Claim<'_>,
        query: &str,
        deadline: Instant,
    ) -> Result<SearchResponse> {
        claim.authorize(&self.runs, None, deadline).await?;
        let (candidates, mut wait) = self.web_candidates().await?;
        let mut attempts = Vec::new();
        for (index, &name) in candidates.iter().enumerate() {
            if let Some(delay) = self.runs.web.wait(name).await {
                wait = Some(wait.map_or(delay, |previous| previous.min(delay)));
                continue;
            }
            // Resolve at dispatch: neither an in-flight first vendor nor a stale
            // eligibility read may let a removed platform key order another call.
            let Some(key) = self.web_key(name).await? else {
                continue;
            };
            let clients = self.runs.web.clients().await?;
            let fleet = claim
                .authorize(&self.runs, Some((name, SEARCH)), deadline)
                .await?;
            let vendors_left = u32::try_from(candidates.len() - index).unwrap_or(1);
            let budget = remaining(deadline)? / vendors_left;
            let start = Instant::now();
            let result =
                tokio::time::timeout(budget, clients.vendor(name).search(&key, query, budget))
                    .await
                    .unwrap_or(Err(WebFailure::Transport));
            self.web_observed(claim, &fleet, name, SEARCH, start, &result)
                .await;
            match result {
                Ok(results) => {
                    return Ok(SearchResponse {
                        vendor: name.name().to_owned(),
                        results,
                    });
                }
                Err(failed) => attempts.push((name, status(&failed))),
            }
        }
        remaining(deadline)?;
        if !attempts.is_empty() {
            return Err(Failure::Vendors(attempts).into());
        }
        Err(wait.map_or(Failure::Unconfigured, Failure::Cooling).into())
    }

    /// The remaining eligible vendor count determines how much of the shared
    /// deadline the first physical call may spend.
    async fn web_candidates(&self) -> Result<(Vec<VendorName>, Option<Duration>)> {
        let mut eligible = Vec::new();
        let mut wait = None;
        for name in [VendorName::Parallel, VendorName::Exa] {
            if self.web_key(name).await?.is_none() {
                continue;
            }
            if let Some(delay) = self.runs.web.wait(name).await {
                wait = Some(wait.map_or(delay, |previous: Duration| previous.min(delay)));
            } else {
                eligible.push(name);
            }
        }
        Ok((eligible, wait))
    }
}
