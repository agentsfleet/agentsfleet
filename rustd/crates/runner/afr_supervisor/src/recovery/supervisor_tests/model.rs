//! The model boundary observes restored history and bytes in a sandbox the
//! restore never parked.
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
    pub(super) resumed: Arc<AtomicUsize>,
    pub(super) parked: Arc<AtomicUsize>,
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
        // The retired warm hold is the one park; the restore parks nothing,
        // and nothing resumes.
        assert_eq!(self.parked.load(Ordering::SeqCst), 1);
        assert_eq!(self.resumed.load(Ordering::SeqCst), 0);
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
