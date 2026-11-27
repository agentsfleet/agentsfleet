//! A private S3-compatible HTTPS fixture, reached through production SDK requests.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{io::AsyncWriteExt as _, net::TcpListener, sync::oneshot, task::JoinHandle};
use tokio_rustls::TlsAcceptor;

mod delete;
mod protocol;
pub(crate) mod tls;

pub(crate) const HOST: &str = "00000000000000000000000000000000.r2.cloudflarestorage.com";

/// What a fixture bucket honours of what a request binds.
#[derive(Clone, Copy)]
struct Honours {
    create_only: bool,
    checksum: bool,
}

pub(crate) struct Server {
    pub client: reqwest::Client,
    pub objects: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    /// Every request served, as its method and path, in order.
    pub requests: Arc<Mutex<Vec<(String, String)>>>,
    stop: Option<oneshot::Sender<()>>,
    worker: Option<JoinHandle<()>>,
}

impl Server {
    pub(crate) async fn start() -> Self {
        Self::serving(Honours {
            create_only: true,
            checksum: true,
        })
        .await
    }

    /// A backend that silently replaces an object a create-only write names.
    pub(crate) async fn replacing() -> Self {
        Self::serving(Honours {
            create_only: false,
            checksum: true,
        })
        .await
    }

    /// A backend that refuses a second create but stores a body unlike the
    /// checksum its request binds.
    pub(crate) async fn ignoring_checksum() -> Self {
        Self::serving(Honours {
            create_only: true,
            checksum: false,
        })
        .await
    }

    /// How many requests of `method` named an object under `prefix`.
    pub(crate) fn count(&self, method: &str, prefix: &str) -> usize {
        self.requests
            .lock()
            .expect("request log")
            .iter()
            .filter(|(seen, path)| seen == method && path.starts_with(prefix))
            .count()
    }

    async fn serving(honours: Honours) -> Self {
        let socket = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("private object listener");
        let address = socket.local_addr().expect("listener address");
        let (root, tls) = tls::server(HOST);
        #[expect(
            clippy::disallowed_methods,
            reason = "fixture pins a generated CA hostname to its own ephemeral listener"
        )]
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(5))
            .add_root_certificate(root)
            .resolve(HOST, address)
            .build()
            .expect("pinned HTTPS client");
        let objects = Arc::new(Mutex::new(BTreeMap::new()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let state = State {
            objects: Arc::clone(&objects),
            requests: Arc::clone(&requests),
            honours,
        };
        let (stop, stopped) = oneshot::channel();
        let worker = tokio::spawn(serve(socket, tls, state, stopped));
        Self {
            client,
            objects,
            requests,
            stop: Some(stop),
            worker: Some(worker),
        }
    }

    pub(crate) async fn finish(mut self) {
        self.stop
            .take()
            .expect("stop handle")
            .send(())
            .expect("worker alive");
        tokio::time::timeout(Duration::from_secs(5), self.worker.take().expect("worker"))
            .await
            .expect("bounded server shutdown")
            .expect("server assertions");
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.abort();
        }
    }
}

/// What the serving task owns: the bucket, its request log and what it honours.
struct State {
    objects: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    requests: Arc<Mutex<Vec<(String, String)>>>,
    honours: Honours,
}

async fn serve(
    socket: TcpListener,
    tls: TlsAcceptor,
    state: State,
    mut stopped: oneshot::Receiver<()>,
) {
    loop {
        let accepted = tokio::select! {
            _ = &mut stopped => break,
            accepted = socket.accept() => accepted,
        };
        let (socket, _) = accepted.expect("SDK connects");
        let mut stream = tls.accept(socket).await.expect("verified TLS handshake");
        let request = protocol::read(&mut stream).await;
        state
            .requests
            .lock()
            .expect("request log")
            .push((request.method.clone(), request.path.clone()));
        let (status, body) = respond(request, &state.objects, state.honours);
        let header = format!(
            "HTTP/1.1 {status} Result\r\nContent-Length: {}\r\nETag: \"fixture-etag\"\r\nLast-Modified: Wed, 04 Nov 2026 00:00:00 GMT\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream
            .write_all(header.as_bytes())
            .await
            .expect("response headers");
        stream.write_all(&body).await.expect("response body");
        stream.shutdown().await.expect("TLS close");
    }
}

fn respond(
    request: protocol::Request,
    owned: &Mutex<BTreeMap<String, Vec<u8>>>,
    honours: Honours,
) -> (u16, Vec<u8>) {
    let mut objects = owned.lock().expect("object state");
    let response = match request.method.as_str() {
        "PUT"
            if honours.create_only
                && request.create_only
                && objects.contains_key(&request.path) =>
        {
            Some((412, vec![]))
        }
        "PUT" if honours.checksum && !checksum_matches(&request) => {
            Some((400, b"BadDigest".to_vec()))
        }
        "PUT" => {
            objects.insert(request.path, request.body);
            Some((200, vec![]))
        }
        "GET" => Some(
            objects
                .get(&request.path)
                .map_or((404, vec![]), |body| (200, body.clone())),
        ),
        "DELETE" => {
            objects.remove(&request.path);
            Some((204, vec![]))
        }
        "POST" => Some((200, delete::objects(&request, &mut objects))),
        _ => None,
    };
    response.expect("unexpected SDK method")
}

/// Whether a PUT's body is the one its bound checksum names; a PUT binding
/// none binds nothing.
fn checksum_matches(request: &protocol::Request) -> bool {
    request.checksum.as_ref().is_none_or(|bound| {
        let digest = hex::decode(af_core::etag::sha256_hex(&request.body)).expect("hex digest");
        *bound == base64::Engine::encode(&base64::prelude::BASE64_STANDARD, digest)
    })
}
