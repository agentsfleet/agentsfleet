//! Exact remote request paths distinguish head validation from default-branch validation.
use super::super::github_authority::WORKFLOW;
use base64::{Engine as _, prelude::BASE64_STANDARD};
use serde_json::{Value, json};

pub(super) fn responses(protected: bool) -> Vec<Value> {
    let repo = json!({"id":101,"full_name":"example/linkwarden","default_branch":"main"});
    let mut replies = vec![
        get(
            "/repos/example/linkwarden/pulls/101",
            &json!({"number":101,"state":"open","merged":false,
            "head":{"ref":"repair","sha":"1".repeat(40),"repo":repo},
            "base":{"ref":"main","sha":"0".repeat(40),"repo":repo}}),
        ),
        get(
            "/repos/example/linkwarden/branches/repair",
            &json!({"protected":protected,"commit":{"sha":"1".repeat(40)}}),
        ),
    ];
    if protected {
        return replies;
    }
    replies.push(get(
        &format!("/repos/example/linkwarden/git/commits/{}", "1".repeat(40)),
        &json!({"sha":"1".repeat(40),"tree":{"sha":"2".repeat(40)}}),
    ));
    replies.push(get("/repos/example/linkwarden/actions/workflows?per_page=17", &json!({"total_count":1,"workflows":[{"id":42,"path":".github/workflows/test.yml","state":"active"}]})));
    files(&mut replies, &"1".repeat(40));
    replies.push(get(
        "/repos/example/linkwarden/branches/main",
        &json!({"commit":{"sha":"0".repeat(40)}}),
    ));
    files(&mut replies, &"0".repeat(40));
    replies
}

fn files(replies: &mut Vec<Value>, head: &str) {
    replies.push(get(
        &format!("/repos/example/linkwarden/contents/.github/workflows?ref={head}"),
        &json!([{"path":".github/workflows/test.yml","sha":"3".repeat(40),"type":"file"}]),
    ));
    replies.push(get(
        &format!("/repos/example/linkwarden/git/blobs/{}", "3".repeat(40)),
        &json!({"content":BASE64_STANDARD.encode(WORKFLOW),"encoding":"base64"}),
    ));
}

fn get(path: &str, response: &Value) -> Value {
    json!({"fixture_request":{"method":"GET","path":path},"response":response})
}
