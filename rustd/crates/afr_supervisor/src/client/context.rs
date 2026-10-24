//! Typed private context requests use the existing authenticated runner transport.
use super::{ControlPlane, Verb};
use crate::Result;
use afd_core::id::Uuid7;
use afd_wire::{
    context::{ContextLoaded, ContextPut},
    paths,
};
use std::borrow::Cow;

fn path(lease: &Uuid7, branch: &str) -> String {
    paths::LEASE_CONTEXT
        .replace("{lease_id}", lease.as_str())
        .replace("{branch_id}", branch)
}

impl ControlPlane {
    pub(crate) async fn context_load(
        &self,
        lease: &Uuid7,
        branch: &str,
        fence: u64,
    ) -> Result<ContextLoaded> {
        let path = Cow::Owned(format!("{}?fencing_token={fence}", path(lease, branch)));
        self.send(Verb::ContextLoad, path, None).await?.decode()
    }
    pub(crate) async fn context_commit(
        &self,
        lease: &Uuid7,
        branch: &str,
        request: &ContextPut,
    ) -> Result<()> {
        self.send_json(
            Verb::ContextCommit,
            Cow::Owned(path(lease, branch)),
            request,
        )
        .await
        .map(drop)
    }
}
