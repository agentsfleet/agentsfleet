//! A model whose turns are scripted, so one lease makes exactly one tool call.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use afd_wire::activity::StreamTextKind;
use afd_wire::lease::LeasePayload;
use afd_wire::policy::ExecutionPolicy;
use afr_providers::{Call, Chunk, Connect, End, Provider, Request, Usage};
use afr_tools::catalog::FILE_READ;
use futures_util::StreamExt as _;
use futures_util::stream::BoxStream;

/// A model whose turns are scripted: a call to `file_read`, then an answer.
#[derive(Debug, Clone)]
pub(super) struct Scripted(pub(super) Arc<AtomicUsize>);

impl Scripted {
    /// The chunks turn `index` streams.
    fn turn(index: usize) -> Vec<Chunk> {
        let spent = Chunk::Usage(Usage {
            input: 2,
            cached_input: 0,
            cache_write: 0,
            cache_write_5m: 0,
            cache_write_1h: 0,
            output: 1,
        });
        let said = if index == 0 {
            Chunk::Call(Call {
                id: "call-1".to_owned(),
                name: FILE_READ.name().to_owned(),
                arguments: serde_json::json!({"path": "README.md"}),
            })
        } else {
            Chunk::Text {
                kind: StreamTextKind::Answer,
                text: "done".to_owned(),
            }
        };
        vec![said, spent, Chunk::End(End::default())]
    }
}

impl Connect for Scripted {
    fn admit(&self, _policy: &ExecutionPolicy<'_>) -> afr_providers::Result<()> {
        Ok(())
    }

    fn connect(&self, _lease: &LeasePayload<'_>) -> afr_providers::Result<Box<dyn Provider>> {
        Ok(Box::new(self.clone()))
    }
}

impl Provider for Scripted {
    fn stream<'a>(&'a self, _request: Request<'a>) -> BoxStream<'a, afr_providers::Result<Chunk>> {
        let index = self.0.fetch_add(1, Ordering::SeqCst);
        futures_util::stream::iter(Self::turn(index).into_iter().map(Ok)).boxed()
    }

    fn accepts_images(&self) -> bool {
        false
    }
}
