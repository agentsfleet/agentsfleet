//! One owned TLS request, observed before the test releases its reply.

use serde_json::Value;
use tokio::{io::{AsyncReadExt as _, AsyncWriteExt as _}, net::TcpListener, sync::oneshot, task::JoinHandle};

pub(super) struct Server {
    pub client: reqwest::Client,
    pub endpoint: String,
    seen: oneshot::Receiver<Value>,
    release: Option<oneshot::Sender<()>>,
    worker: JoinHandle<()>,
}

impl Server {
    pub async fn start(reply: Value) -> Self {
        let socket = TcpListener::bind("127.0.0.1:0").await.expect("isolated decision port");
        let address = socket.local_addr().expect("bound address");
        let (root, tls) = super::tls::server();
        let client = super::tls::client(root, address);
        let (sent, seen) = oneshot::channel();
        let (release, released) = oneshot::channel();
        let worker = tokio::spawn(async move {
            let (socket, _) = socket.accept().await.expect("decision connects");
            let mut stream = tls.accept(socket).await.expect("verified TLS handshake");
            let body = read(&mut stream).await;
            sent.send(body).expect("scenario awaits decision request");
            released.await.expect("scenario releases response");
            let body = serde_json::to_string(&reply).expect("decision fixture JSON");
            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            stream.write_all(response.as_bytes()).await.expect("response write");
            stream.shutdown().await.expect("TLS close");
        });
        Self { client, endpoint: format!("https://{}:{}/v1/systemone", super::tls::HOST, address.port()), seen, release: Some(release), worker }
    }

    pub async fn received(&mut self) -> Value {
        tokio::time::timeout(std::time::Duration::from_secs(5), &mut self.seen).await
            .expect("decision arrives within fixture budget").expect("owned server")
    }

    pub async fn finish(mut self) {
        self.release.take().expect("release sender").send(()).expect("response remains owned");
        tokio::time::timeout(std::time::Duration::from_secs(5), &mut self.worker).await
            .expect("server terminates").expect("server task succeeds");
    }
}

async fn read(stream: &mut tokio_rustls::server::TlsStream<tokio::net::TcpStream>) -> Value {
    let mut request = Vec::new();
    let (offset, length) = loop {
        let mut buffer = [0; 4096];
        let count = stream.read(&mut buffer).await.expect("request bytes");
        assert_ne!(count, 0, "complete HTTP request");
        request.extend_from_slice(&buffer[..count]);
        if let Some(offset) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            let header = std::str::from_utf8(&request[..offset]).expect("HTTP header");
            let length = header.lines().find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<usize>().expect("content length"))
            }).expect("request content length");
            break (offset + 4, length);
        }
    };
    while request.len() < offset + length {
        let mut buffer = [0; 4096];
        let count = stream.read(&mut buffer).await.expect("remaining body");
        assert_ne!(count, 0, "complete request body");
        request.extend_from_slice(&buffer[..count]);
    }
    serde_json::from_slice(&request[offset..offset + length]).expect("daemon decision request")
}

impl Drop for Server {
    fn drop(&mut self) {
        self.worker.abort();
    }
}
