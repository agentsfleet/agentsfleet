//! Only the external model is scripted; the actual agent owns task preparation/history.
use af_providers::{Connect, Provider};
use futures_util::{StreamExt as _, stream::BoxStream};
use guffbolt::{Chunk, End, Request};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Debug, Clone, Default)]
pub(super) struct Model(pub(super) Arc<AtomicUsize>);
impl Connect for Model {
    fn admit(&self, _: &af_wire::policy::ExecutionPolicy<'_>) -> af_providers::Result<()> {
        Ok(())
    }
    fn connect(&self, _: &af_wire::lease::LeasePayload<'_>) -> af_providers::Result<Box<Provider>> {
        Ok(Box::new(self.clone()))
    }
}
impl guffbolt::Model for Model {
    type Error = af_providers::Error;

    fn measure(&self, request: Request<'_>) -> af_providers::Result<u64> {
        af_providers::testing::measure(request)
    }

    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, af_providers::Result<Chunk>> {
        assert!(request.instructions.contains("Native task scope"));
        assert_eq!(request.tools.len(), 0);
        self.0.fetch_add(1, Ordering::SeqCst);
        futures_util::stream::iter([
            Ok(Chunk::Text {
                kind: guffbolt::TextKind::Answer,
                text: "Progress is ready to save.".to_owned(),
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
