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
mod tls;

pub(super) const HOST: &str = "00000000000000000000000000000000.r2.cloudflarestorage.com";

pub(super) struct Server {
    pub client: reqwest::Client,
    pub objects: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    stop: Option<oneshot::Sender<()>>,
    worker: Option<JoinHandle<()>>,
}

impl Server {
    pub(super) async fn start() -> Self {
        let socket = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("private object listener");
        let address = socket.local_addr().expect("listener address");
        let (root, tls) = tls::server();
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
        let owned = Arc::clone(&objects);
        let (stop, stopped) = oneshot::channel();
        let worker = tokio::spawn(serve(socket, tls, owned, stopped));
        Self {
            client,
            objects,
            stop: Some(stop),
            worker: Some(worker),
        }
    }

    pub(super) async fn finish(mut self) {
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

async fn serve(
    socket: TcpListener,
    tls: TlsAcceptor,
    owned: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    mut stopped: oneshot::Receiver<()>,
) {
    loop {
        let accepted = tokio::select! {
            () = async { let _ = (&mut stopped).await; } => break,
            accepted = socket.accept() => accepted,
        };
        let (socket, _) = accepted.expect("SDK connects");
        let mut stream = tls.accept(socket).await.expect("verified TLS handshake");
        let request = protocol::read(&mut stream).await;
        let (status, body) = respond(request, &owned);
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

fn respond(request: protocol::Request, owned: &Mutex<BTreeMap<String, Vec<u8>>>) -> (u16, Vec<u8>) {
    let mut objects = owned.lock().expect("object state");
    let response = match request.method.as_str() {
        "PUT" if request.create_only && objects.contains_key(&request.path) => Some((412, vec![])),
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
