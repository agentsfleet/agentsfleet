//! The run's egress guard as the web tools' backend.

use guffbolt::CallRef;
use guffbolt::tools::outbound::{OutboundFuture, Response};
use guffbolt::tools::web::{SearchBackend, SearchResult};

use crate::egress::Egressed;

impl SearchBackend for Egressed<'_, '_> {
    fn search<'a>(
        &'a self,
        call: &'a CallRef,
        query: &'a str,
    ) -> OutboundFuture<'a, Vec<SearchResult>> {
        Box::pin(Egressed::search(self, &call.call_id, query))
    }

    fn extract<'a>(
        &'a self,
        call: &'a CallRef,
        url: &'a str,
    ) -> OutboundFuture<'a, (Response, Option<String>)> {
        Box::pin(self.fetch(&call.call_id, url))
    }
}
