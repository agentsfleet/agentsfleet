//! The model boundary observes restored history, bytes and a successful thaw.
use super::fixture::{CONTENT, FILE, PREFIX};
use af_providers::{Connect, Provider};
use af_wire::run::RunPayload;
use futures_util::{StreamExt as _, stream::BoxStream};
use guffbolt::{Chunk, End, Message, Request};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
#[derive(Debug, Clone)]
pub(super) struct ObservingModel {
    pub(super) root: PathBuf,
    pub(super) thawed: Arc<AtomicUsize>,
    pub(super) frozen: Arc<AtomicUsize>,
    pub(super) calls: Arc<AtomicUsize>,
}

impl Connect for ObservingModel {
    fn admit(&self, _: &af_wire::policy::ExecutionPolicy<'_>) -> af_providers::Result<()> {
        Ok(())
    }
    fn connect(&self, _: &RunPayload<'_>) -> af_providers::Result<Box<Provider>> {
        Ok(Box::new(self.clone()))
    }
}

impl guffbolt::Model for ObservingModel {
    type Error = af_providers::Error;

    fn measure(&self, request: Request<'_>) -> af_providers::Result<u64> {
        af_providers::testing::measure(request)
    }

    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, af_providers::Result<Chunk>> {
        let thawed = self.thawed.load(Ordering::SeqCst);
        assert!(thawed >= 1);
        // The retired warm hold accounts for the one freeze without a thaw.
        assert_eq!(self.frozen.load(Ordering::SeqCst), thawed + 1);
        assert_eq!(
            std::fs::read(self.root.join(FILE)).ok().as_deref(),
            Some(CONTENT)
        );
        assert!(
            request
                .messages
                .iter()
                .any(|item| matches!(item, Message::User(text) if text.contains(PREFIX)))
        );
        self.calls.fetch_add(1, Ordering::SeqCst);
        futures_util::stream::iter([
            Ok(Chunk::Text {
                kind: guffbolt::TextKind::Answer,
                text: "Resumed.".to_owned(),
            }),
            Ok(Chunk::Usage(guffbolt::Usage {
                input: 10,
                output: 2,
                ..guffbolt::Usage::default()
            })),
            Ok(Chunk::End(End::default())),
        ])
        .boxed()
    }
    fn accepts_images(&self) -> bool {
        false
    }
}
