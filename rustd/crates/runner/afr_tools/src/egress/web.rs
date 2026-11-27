//! Web reads under the fleet policy and the original call's monotonic deadline.

use std::{borrow::Cow, time::Duration};

use af_core::{error_code, problem::Problem};
use af_egress::{Draft, Placement, Refusal};
use guffbolt::tools::outbound::{Refused, Response};
use guffbolt::tools::web::SearchResult;
use tokio::time::{Instant, timeout, timeout_at};

use super::{Egressed, NO_TRANSPORT, get_chain, read_expired, refused, response};
use crate::catalog::{WEB_FETCH, WEB_SEARCH};
use crate::runtime::ToolErrorCode;
use crate::verbs::{Unanswered, WebUnanswered};

const NETWORK_RESTRICTED: guffbolt::ErrorCode = guffbolt::ErrorCode::new("network_restricted");
const PRIVATE_INPUT: &str =
    "This input contains a protected value and cannot be sent to a web vendor.";

impl Egressed<'_, '_> {
    pub(crate) async fn search(
        &self,
        call_id: &str,
        query: &str,
    ) -> Result<Vec<SearchResult>, Refused> {
        let number = self.call_number(call_id)?;
        let query_allowed = async {
            let mut guard = self.run.egress.lock().await;
            if !guard.public_web() {
                return Err(restricted());
            }
            if matches!(
                guard.web_text(query).map_err(|e| refused(
                    WEB_SEARCH.name(),
                    self.run.run_id,
                    &e
                ))?,
                Cow::Owned(_)
            ) {
                return Err(Refused::new(
                    ToolErrorCode::InvalidArguments.into(),
                    PRIVATE_INPUT,
                ));
            }
            drop(guard);
            let answer = self
                .run
                .verbs
                .web_search(number, query)
                .await
                .map_err(verb_refused)?;
            let mut guard = self.run.egress.lock().await;
            let mut results = Vec::with_capacity(answer.results.len());
            for row in answer.results {
                if let Some(safe) = clean_result(&mut guard, row)
                    .map_err(|e| refused(WEB_SEARCH.name(), self.run.run_id, &e))?
                {
                    results.push(safe);
                }
            }
            Ok(results)
        };
        timeout(self.run.verbs.web_timeout(), query_allowed)
            .await
            .map_err(|_| read_expired(WEB_SEARCH.name(), self.run.run_id))?
    }

    pub(crate) async fn fetch(
        &self,
        call_id: &str,
        url: &str,
    ) -> Result<(Response, Option<String>), Refused> {
        let deadline = Instant::now() + self.run.verbs.web_timeout();
        timeout_at(deadline, self.routed_read(call_id, url, deadline))
            .await
            .map_err(|_| read_expired(WEB_FETCH.name(), self.run.run_id))?
    }

    async fn routed_read(
        &self,
        call_id: &str,
        url: &str,
        deadline: Instant,
    ) -> Result<(Response, Option<String>), Refused> {
        let tool = WEB_FETCH.name();
        let run = self.run;
        let mut guard = run.egress.lock().await;
        let local = guard.fetch_locally(url).map_err(|failure| {
            if failure.refusal() == Some(Refusal::HostNotAllowed) {
                Refused::new(
                    NETWORK_RESTRICTED,
                    format!("{}. Allow all enables public web reads.", failure.detail()),
                )
            } else {
                refused(tool, run.run_id, &failure)
            }
        })?;
        if !local
            && matches!(
                guard
                    .web_text(url)
                    .map_err(|e| refused(tool, run.run_id, &e))?,
                Cow::Owned(_)
            )
        {
            return Err(Refused::new(
                ToolErrorCode::InvalidArguments.into(),
                PRIVATE_INPUT,
            ));
        }
        drop(guard);
        if !local {
            let number = self.call_number(call_id)?;
            match run.verbs.web_extract(number, url).await {
                Ok(page) => {
                    return Ok((
                        Response {
                            status: 200,
                            body: run
                                .egress
                                .lock()
                                .await
                                .web_text(&page.content)
                                .map_err(|e| refused(tool, run.run_id, &e))?
                                .into_owned(),
                            content_type: Some("text/markdown".to_owned()),
                            ..Response::default()
                        },
                        None,
                    ));
                }
                Err(WebUnanswered::Unanswered(why)) => return Err(verb_refused(why)),
                Err(WebUnanswered::LocalReadPermitted) => {}
            }
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        self.local_read(call_id, url, remaining).await
    }

    async fn local_read(
        &self,
        call_id: &str,
        url: &str,
        remaining: Duration,
    ) -> Result<(Response, Option<String>), Refused> {
        let tool = WEB_FETCH.name();
        let transport = self
            .transport
            .ok_or_else(|| Refused::new(ToolErrorCode::UpstreamUnreachable.into(), NO_TRANSPORT))?;
        if remaining.is_zero() {
            return Err(read_expired(tool, self.run.run_id));
        }
        let draft = Draft {
            method: "GET".to_owned(),
            url: url.to_owned(),
            headers: Vec::new(),
            body: None,
            placement: Placement::Nowhere,
        };
        let (page, origin) = timeout(
            remaining.min(transport.limits().request_timeout),
            get_chain(tool, transport, self.run, draft, Some(call_id)),
        )
        .await
        .map_err(|_| read_expired(tool, self.run.run_id))??;
        let mut guard = self.run.egress.lock().await;
        let origin = match origin {
            Some(origin)
                if matches!(
                    guard
                        .web_text(&origin)
                        .map_err(|e| refused(tool, self.run.run_id, &e))?,
                    Cow::Borrowed(_)
                ) =>
            {
                Some(origin)
            }
            _ => None,
        };
        Ok((response(page), origin))
    }

    fn call_number(&self, call_id: &str) -> Result<u64, Refused> {
        self.run.calls.of(call_id).ok_or_else(|| {
            Refused::new(
                ToolErrorCode::AgentsfleetdRefused.into(),
                "The web call has no open run record.",
            )
        })
    }
}

fn clean_result(
    guard: &mut af_egress::Egress<'_>,
    row: af_wire::web::SearchResult,
) -> af_egress::Result<Option<SearchResult>> {
    if matches!(guard.web_text(&row.url)?, Cow::Owned(_)) {
        return Ok(None);
    }
    let Some(safe_url) = af_wire::tool_trace::WebSource::display_url(&row.url) else {
        return Ok(None);
    };
    let protected_query = safe_url.as_str() != row.url;
    let mut clean = |text: String| {
        let text = if protected_query {
            text.replace(&row.url, safe_url.as_str())
        } else {
            text
        };
        match guard.web_text(&text)? {
            Cow::Borrowed(_) => Ok(text),
            Cow::Owned(masked) => Ok(masked),
        }
    };
    Ok(Some(SearchResult {
        url: safe_url.to_string(),
        title: row
            .title
            .filter(|title| {
                !protected_query && af_wire::tool_trace::WebSource::displayable_title(title)
            })
            .map(&mut clean)
            .transpose()?,
        published: row.published.map(&mut clean).transpose()?,
        excerpts: row
            .excerpts
            .into_iter()
            .map(clean)
            .collect::<af_egress::Result<_>>()?,
    }))
}

fn restricted() -> Refused {
    Refused::new(
        NETWORK_RESTRICTED,
        Problem::of(error_code::WEB_NETWORK_RESTRICTED).hint(),
    )
}

fn verb_refused(why: Unanswered) -> Refused {
    if why == Unanswered::Refused(Some(error_code::WEB_NETWORK_RESTRICTED)) {
        return restricted();
    }
    Refused::new(why.tool_code().into(), why.detail())
}

#[cfg(test)]
#[path = "web_tests.rs"]
mod tests;
