//! The external App API validates the real broker's signed, repository-scoped mint.
use super::Request;
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) struct Exchange {
    installation: u32,
    key: jsonwebtoken::DecodingKey,
    write: bool,
    reads: usize,
    mints: usize,
}

#[derive(Clone, Deserialize)]
struct Claims {
    iss: u64,
    iat: i64,
    exp: i64,
}

impl Exchange {
    pub(super) fn new(installation: u32, public_key: &str, write: bool) -> Self {
        Self {
            installation,
            key: jsonwebtoken::DecodingKey::from_rsa_pem(public_key.as_bytes())
                .expect("generated public key"),
            write,
            reads: 0,
            mints: 0,
        }
    }

    pub(super) fn respond(&mut self, request: &Request) -> Option<Value> {
        let root = format!("/app/installations/{}", self.installation);
        let token_path = format!("{root}/access_tokens");
        if request.path != root && request.path != token_path {
            return None;
        }
        self.signature(request);
        if request.path == root {
            assert_eq!(request.method, "GET");
            assert!(request.body.is_empty());
            self.reads += 1;
            return Some(
                json!({"permissions":{"contents":"write","pull_requests":"write",
                "actions":"read","checks":"read","metadata":"read"}}),
            );
        }
        assert_eq!(request.method, "POST");
        assert_eq!(
            self.reads,
            self.mints + 1,
            "installation proof precedes mint"
        );
        let body: Value = serde_json::from_slice(&request.body).expect("scoped token request");
        let access = body["permissions"]["contents"]
            .as_str()
            .expect("contents permission");
        assert!(access == "read" || (self.write && access == "write"));
        assert_eq!(
            body,
            json!({"repositories":["linkwarden"],
            "permissions":{"contents":access,"pull_requests":access,"actions":"read","checks":"read"}})
        );
        self.mints += 1;
        Some(
            json!({"token":"native-test-read-token","permissions":body["permissions"],
            "repositories":[{"full_name":"example/linkwarden"}]}),
        )
    }

    fn signature(&self, request: &Request) {
        let jwt = request
            .authorization
            .strip_prefix("Bearer ")
            .expect("App JWT authorization");
        let validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
        let claims = jsonwebtoken::decode::<Claims>(jwt, &self.key, &validation)
            .expect("real App signature and expiry")
            .claims;
        assert_eq!(claims.iss, 7);
        assert_eq!(claims.exp - claims.iat, 600);
    }

    pub(super) fn finish(&self) {
        assert!(self.mints > 0, "real broker reached the external App API");
        assert_eq!(
            self.reads, self.mints,
            "every installation read led to a scoped mint"
        );
    }
}
