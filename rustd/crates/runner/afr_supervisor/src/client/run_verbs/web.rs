//! Web dispatches are purchased attempts. Send each exactly once even if the
//! daemon is unavailable or a response is lost.
use af_core::id::Uuid7;
use af_wire::paths::{RUN_EXTRACTS_SUFFIX, RUN_SEARCHES_SUFFIX};
use af_wire::web::{ExtractRequest, ExtractResponse, SearchRequest, SearchResponse};

use crate::client::{ControlPlane, Verb, run_path};
use crate::error::Result;

impl ControlPlane {
    pub(crate) async fn web_search(
        &self,
        run_id: &Uuid7,
        fencing_token: u64,
        call_number: u64,
        query: &str,
    ) -> Result<SearchResponse> {
        let path = run_path(run_id, RUN_SEARCHES_SUFFIX);
        let request = SearchRequest {
            fencing_token,
            call_number,
            query: query.to_owned(),
        };
        self.send_json(Verb::WebSearch, path, &request)
            .await?
            .decode()
    }

    pub(crate) async fn web_extract(
        &self,
        run_id: &Uuid7,
        fencing_token: u64,
        call_number: u64,
        url: &str,
    ) -> Result<ExtractResponse> {
        let path = run_path(run_id, RUN_EXTRACTS_SUFFIX);
        let request = ExtractRequest {
            fencing_token,
            call_number,
            url: url.to_owned(),
        };
        self.send_json(Verb::WebExtract, path, &request)
            .await?
            .decode()
    }
}
