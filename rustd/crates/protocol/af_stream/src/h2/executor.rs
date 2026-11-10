use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::Result;

#[derive(Debug, Clone)]
pub(super) struct Executor {
    stop: CancellationToken,
    tasks: TaskTracker,
}

impl Executor {
    pub(super) fn new(stop: CancellationToken) -> Self {
        Self {
            stop,
            tasks: TaskTracker::new(),
        }
    }

    pub(super) async fn run(&self, connection: impl Future<Output = Result<()>>) -> Result<()> {
        let _guard = self.stop.clone().drop_guard();
        let result = tokio::select! {
            biased;
            () = self.stop.cancelled() => Ok(()),
            result = connection => result,
        };
        self.stop.cancel();
        self.tasks.close();
        self.tasks.wait().await;
        result
    }
}

impl<F: Future<Output = ()> + Send + 'static> hyper::rt::Executor<F> for Executor {
    fn execute(&self, fut: F) {
        let stop = self.stop.clone();
        // The tracker retains worker lifetimes after Hyper releases their handles.
        let _worker = self.tasks.spawn(async move {
            tokio::select! { biased; () = stop.cancelled() => {}, () = fut => {} }
        });
    }
}
