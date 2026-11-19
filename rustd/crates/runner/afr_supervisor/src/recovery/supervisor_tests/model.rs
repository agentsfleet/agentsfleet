//! The model boundary observes restored history, bytes and a successful thaw.
use super::fixture::{CONTENT, FILE, PREFIX};
use af_providers::{Chunk, Connect, End, Message, Provider, Request};
use af_wire::lease::LeasePayload;
use futures_util::{StreamExt as _, stream::BoxStream};
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
    fn connect(&self, _: &LeasePayload<'_>) -> af_providers::Result<Box<dyn Provider>> {
        Ok(Box::new(self.clone()))
    }
}

impl Provider for ObservingModel {
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
                kind: af_wire::activity::StreamTextKind::Answer,
                text: "Resumed.".to_owned(),
            }),
            Ok(Chunk::Usage(af_providers::Usage {
                input: 10,
                output: 2,
                ..af_providers::Usage::default()
            })),
            Ok(Chunk::End(End::default())),
        ])
        .boxed()
    }
    fn accepts_images(&self) -> bool {
        false
    }
}
