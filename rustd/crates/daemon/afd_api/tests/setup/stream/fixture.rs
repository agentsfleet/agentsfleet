//! Each stream owns a real server, authenticated account and cleanup lane.

use super::super::{fixture::Fixture, json_body, mint, send};
use af_core::id::Uuid7;
use http::{Method, StatusCode};
use tokio::{net::TcpListener, task::JoinHandle};

pub(super) struct Open {
    pub(super) fixture: Fixture,
    /// The router the stream is served from, and every write a test sends goes through,
    /// so a writer's notice reaches the open stream.
    pub(super) router: axum::Router,
    /// The in-process bus's server: what a test publishes as another replica would.
    pub(super) bus: afd_bus::test_util::Server,
    pub(super) item: String,
    pub(super) draft: Uuid7,
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
        let (handle, bus) = afd_bus::test_util::detached().expect("an in-process bus opens");
        let router = fixture
            .services(&afd_auth::scope::Scope::ALL)
            .with_live_bus(handle)
            .router();
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
        let draft = Uuid7::parse(json_body(created).await["id"].as_str().expect("draft"))
            .expect("the draft id is a UUIDv7");
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("private listener");
        let address = listener.local_addr().expect("listener address");
        let served = router.clone();
        let server = tokio::spawn(async move {
            axum::serve(listener, served).await.expect("server");
        });
        let response = reqwest::Client::new()
            .get(format!("http://{address}{item}/turns/stream"))
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
            router,
            bus,
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

    /// Removes the authenticated person and publishes the account's members notice, as
    /// a member removal does after its commit.
    pub(super) async fn revoke(&self) {
        sqlx::query("DELETE FROM identity.users WHERE id=$1")
            .bind(&self.fixture.owner)
            .execute(
                &mut *self
                    .fixture
                    .db
                    .acquire()
                    .await
                    .expect("revocation connection"),
            )
            .await
            .expect("revoke the authenticated person");
        self.bus
            .publish(&afd_bus::Subject::Members(self.fixture.tenant.clone()), "");
    }

    pub(super) async fn finish(self) {
        drop(self.response);
        self.server.abort();
        // An aborted task answers `Cancelled`; the await only waits it out.
        drop(self.server.await);
        self.fixture.cleanup().await;
    }
}
