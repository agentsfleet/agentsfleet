//! A loopback Atlassian, answering the two calls one Jira connect makes.
//!
//! One listener for both, because `Exchange::pointed_at` moves every host a
//! connect dials to the same origin — so the token POST and the site listing
//! arrive here and are told apart by their PATH, exactly as the real vendor
//! tells them apart. A fake per call would not be a fake of the thing under
//! test: routing by path is the property the pin has.
//!
//! It records the code it was handed, which is what lets the round trip assert
//! that the value the callback carried is the value that reached the vendor —
//! a handoff that type-checks whether or not it is right.

use std::sync::Arc;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

/// The access token this vendor issues.
pub(crate) const ACCESS_TOKEN: &str = "fixture-jira-access-token";

/// The refresh token beside it.
pub(crate) const REFRESH_TOKEN: &str = "fixture-jira-refresh-token";

/// The site the grant is scoped to.
pub(crate) const CLOUD_ID: &str = "fixture-cloud-id";

/// That site's URL.
pub(crate) const SITE_URL: &str = "https://fixture.atlassian.net";

/// That site's human name, which becomes the grant's label.
pub(crate) const SITE_NAME: &str = "Fixture Jira";

/// How long the issued token lasts, in seconds.
const EXPIRES_IN: i64 = 3600;

/// The path Atlassian lists a token's accessible sites on.
const SITES_PATH: &str = "/oauth/token/accessible-resources";

/// A loopback Atlassian, serving until it is dropped.
pub(crate) struct FakeAtlassian {
    /// The origin a pinned exchange dials.
    base: String,
    /// The `code` form field the token POST carried, once one arrives.
    redeemed: Arc<Mutex<Option<String>>>,
    /// The accept loop, aborted on drop.
    serving: tokio::task::JoinHandle<()>,
}

impl FakeAtlassian {
    /// Binds a loopback port and serves both calls until dropped.
    pub(crate) async fn serving() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port is available");
        let port = listener.local_addr().expect("the listener is bound").port();
        let redeemed = Arc::new(Mutex::new(None));
        let recorder = Arc::clone(&redeemed);

        let serving = tokio::spawn(async move {
            while let Ok((mut socket, _peer)) = listener.accept().await {
                let mut buffer = vec![0_u8; 8192];
                let read = socket.read(&mut buffer).await.unwrap_or(0);
                let received =
                    String::from_utf8_lossy(buffer.get(..read).unwrap_or_default()).into_owned();

                let body = if received.contains(SITES_PATH) {
                    sites()
                } else {
                    if let Some(code) = form_field(&received, "code") {
                        *recorder.lock().await = Some(code);
                    }
                    token()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                     content-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _written = socket.write_all(response.as_bytes()).await;
                let _flushed = socket.shutdown().await;
            }
        });

        Self {
            base: format!("http://127.0.0.1:{port}"),
            redeemed,
            serving,
        }
    }

    /// The origin every pinned call lands on.
    pub(crate) fn base(&self) -> &str {
        &self.base
    }

    /// The authorization code this vendor was handed, if it was handed one.
    pub(crate) async fn redeemed_code(&self) -> Option<String> {
        self.redeemed.lock().await.clone()
    }
}

impl Drop for FakeAtlassian {
    fn drop(&mut self) {
        self.serving.abort();
    }
}

/// The token endpoint's answer.
fn token() -> String {
    format!(
        r#"{{"access_token":"{ACCESS_TOKEN}","refresh_token":"{REFRESH_TOKEN}","expires_in":{EXPIRES_IN},"token_type":"Bearer"}}"#
    )
}

/// The site listing's answer: one site, as Atlassian shapes it.
fn sites() -> String {
    format!(r#"[{{"id":"{CLOUD_ID}","name":"{SITE_NAME}","url":"{SITE_URL}"}}]"#)
}

/// One `application/x-www-form-urlencoded` field out of a request.
fn form_field(request: &str, name: &str) -> Option<String> {
    let body = request.split("\r\n\r\n").nth(1)?;
    body.split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _value)| *key == name)
        .map(|(_key, value)| value.replace('+', " "))
}
