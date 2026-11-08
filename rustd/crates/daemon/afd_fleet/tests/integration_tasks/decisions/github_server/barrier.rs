//! A physical response barrier exposes the committed native claim/network window.
use std::sync::Mutex;
use tokio::sync::oneshot;

struct Pending {
    path: String,
    entered: oneshot::Sender<()>,
    release: oneshot::Receiver<()>,
}

#[derive(Default)]
pub(super) struct Barrier(Mutex<Option<Pending>>);

impl Barrier {
    pub(super) fn hold(&self, path: &str) -> (oneshot::Receiver<()>, oneshot::Sender<()>) {
        let (entered, seen) = oneshot::channel();
        let (release, resume) = oneshot::channel();
        let mut pending = self.0.lock().expect("response barrier controller");
        assert!(pending.is_none(), "one response barrier per server");
        *pending = Some(Pending {
            path: path.to_owned(),
            entered,
            release: resume,
        });
        (seen, release)
    }

    pub(super) async fn wait(&self, path: &str) {
        let pending = {
            let mut pending = self.0.lock().expect("response barrier");
            if pending.as_ref().is_some_and(|held| held.path == path) {
                pending.take()
            } else {
                None
            }
        };
        if let Some(pending) = pending {
            pending
                .entered
                .send(())
                .expect("race controller observes request");
            pending
                .release
                .await
                .expect("race controller releases response");
        }
    }
}
