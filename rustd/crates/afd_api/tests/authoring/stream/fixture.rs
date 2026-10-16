//! Each stream owns a real server, authenticated account and cleanup lane.

use super::super::{fixture::Fixture, json_body, mint, send};
use http::{Method, StatusCode};
use tokio::{net::TcpListener, task::JoinHandle};

pub(super) struct Open {
    pub(super) fixture: Fixture,
    pub(super) item: String,
    pub(super) draft: String,
    pub(super) response: reqwest::Response,
    server: JoinHandle<()>,
}

impl Open {
    #[expect(
        clippy::disallowed_methods,
        reason = "a plain client reads the stream from the loopback server this fixture booted; a proxy rule and a deadline are not what the stream tests prove"
    )]
    pub(super) async fn create() -> Self {
        let fixture = Fixture::create().await;
        let router = fixture.router();
        let created = send(
            &router,
            Method::POST,
            &fixture.collection(),
            Some(&fixture.token),
            &fixture.request(&mint()).to_string(),
        )
        .await;
        assert_eq!(created.status(), StatusCode::CREATED);
        let item = created.headers()[http::header::LOCATION]
            .to_str()
            .expect("location")
            .to_owned();
        let draft = json_body(created).await["id"]
            .as_str()
            .expect("draft")
            .to_owned();
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("private listener");
        let address = listener.local_addr().expect("listener address");
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.expect("server");
        });
        let response = reqwest::Client::new()
            .get(format!("http://{address}{item}/events/stream"))
            .bearer_auth(&fixture.token)
            .send()
            .await
            .expect("stream response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[http::header::CONTENT_TYPE],
            "text/event-stream"
        );
        assert!(
            response.headers()[http::header::CACHE_CONTROL]
                .to_str()
                .expect("cache policy")
                .contains("no-store")
        );
        Self {
            fixture,
            item,
            draft,
            response,
            server,
        }
    }

    pub(super) async fn frame(&mut self) -> String {
        let bytes = tokio::time::timeout(super::DELIVERY, self.response.chunk())
            .await
            .expect("frame deadline")
            .expect("read frame")
            .expect("open stream");
        String::from_utf8(bytes.to_vec()).expect("UTF-8 frame")
    }

    pub(super) async fn closed(&mut self) {
        assert!(
            tokio::time::timeout(super::DELIVERY, self.response.chunk())
                .await
                .expect("revocation deadline")
                .expect("clean stream end")
                .is_none()
        );
    }

    pub(super) async fn finish(self) {
        drop(self.response);
        self.server.abort();
        let _ = self.server.await;
        self.fixture.cleanup().await;
    }
}
