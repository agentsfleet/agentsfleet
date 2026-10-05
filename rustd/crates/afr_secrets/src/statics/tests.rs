use serde_json::json;

use super::StaticSecrets;

#[test]
fn test_static_secrets_read_fields_and_host() {
    let map = json!({"github": {"token": "ghp_1", "host": "api.github.com", "n": 7}});
    let secrets = StaticSecrets::new(Some(&map));

    assert_eq!(secrets.field("github", "token"), Some("ghp_1"));
    assert_eq!(secrets.host("github"), Some("api.github.com"));
    assert_eq!(
        secrets.field("github", "n"),
        None,
        "a number is no string field"
    );
    assert_eq!(secrets.field("slack", "token"), None);
    assert!(secrets.contains("github"));
    assert!(!secrets.contains("slack"));
}

#[test]
fn test_static_secret_values_skip_the_host_and_non_strings() {
    let map = json!({
        "github": {"token": "ghp_1", "host": "api.github.com"},
        "fly": {"api_token": "fly_1", "ttl": 30},
        "broken": "not an object"
    });

    let mut values: Vec<_> = StaticSecrets::new(Some(&map)).values().collect();
    values.sort();

    assert_eq!(
        values,
        vec![
            ("fly.api_token".to_owned(), "fly_1"),
            ("github.token".to_owned(), "ghp_1"),
        ]
    );
}

#[test]
fn test_static_secrets_tolerate_an_absent_or_odd_map() {
    for map in [None, Some(json!([1, 2])), Some(json!("x"))] {
        let secrets = StaticSecrets::new(map.as_ref());
        assert_eq!(secrets.values().count(), 0);
        assert!(!secrets.contains("github"));
    }
}

/// A map read without a `Value` around it yields what the policy view does,
/// so the daemon and the runner mask the same names.
#[test]
fn test_a_map_view_reads_as_the_policy_view() {
    let policy = json!({"github": {"token": "ghp_1", "host": "api.github.com"}});
    let map = policy.as_object().cloned().unwrap_or_default();

    let from_map: Vec<_> = StaticSecrets::of_map(&map).values().collect();
    let from_policy: Vec<_> = StaticSecrets::new(Some(&policy)).values().collect();

    assert_eq!(from_map, from_policy);
    assert_eq!(from_map, vec![("github.token".to_owned(), "ghp_1")]);
}
