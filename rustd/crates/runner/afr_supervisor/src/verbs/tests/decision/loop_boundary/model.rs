//! Only the external model is scripted; the actual agent owns task preparation/history.
use af_providers::{Chunk, Connect, End, Provider, Request};
use futures_util::{StreamExt as _, stream::BoxStream};
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
    fn connect(
        &self,
        _: &af_wire::lease::LeasePayload<'_>,
    ) -> af_providers::Result<Box<dyn Provider>> {
        Ok(Box::new(self.clone()))
    }
}
impl Provider for Model {
    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, af_providers::Result<Chunk>> {
        assert!(request.instructions.contains("Native task scope"));
        assert_eq!(request.tools.len(), 0);
        self.0.fetch_add(1, Ordering::SeqCst);
        futures_util::stream::iter([
            Ok(Chunk::Text {
                kind: af_wire::activity::StreamTextKind::Answer,
                text: "Progress is ready to save.".to_owned(),
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
