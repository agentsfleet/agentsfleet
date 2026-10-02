#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::path::Path;

use afd_core::env::MapEnv;

use super::{Config, DEFAULT_STORAGE_HOME, ENV_API_URL, ENV_RUNNER_TOKEN, ENV_STORAGE_HOME};

const URL: &str = "https://api.example.test";
const TOKEN: &str = "agt_r_secret";

fn read(pairs: &[(&str, &str)]) -> crate::Result<Config> {
    Config::from_env(&MapEnv::from_pairs(pairs.iter().copied()))
}

#[test]
fn a_complete_environment_reads_with_the_default_home() {
    let config = read(&[(ENV_API_URL, URL), (ENV_RUNNER_TOKEN, TOKEN)]).unwrap();

    assert_eq!(config.api_url().as_str(), "https://api.example.test/");
    assert_eq!(config.token().expose(), TOKEN);
    assert_eq!(config.storage_home(), Path::new(DEFAULT_STORAGE_HOME));
}

#[test]
fn a_path_prefix_survives_as_a_directory_routes_join_under() {
    let config = read(&[
        (ENV_API_URL, "https://h.test/api"),
        (ENV_RUNNER_TOKEN, TOKEN),
    ])
    .unwrap();
    let joined = config.api_url().join("v1/runners/me/leases").unwrap();

    assert_eq!(joined.as_str(), "https://h.test/api/v1/runners/me/leases");
}

#[test]
fn a_set_storage_home_wins() {
    let config = read(&[
        (ENV_API_URL, URL),
        (ENV_RUNNER_TOKEN, TOKEN),
        (ENV_STORAGE_HOME, "/srv/r"),
    ])
    .unwrap();

    assert_eq!(config.storage_home(), Path::new("/srv/r"));
}

#[test]
fn the_token_never_prints() {
    let config = read(&[(ENV_API_URL, URL), (ENV_RUNNER_TOKEN, TOKEN)]).unwrap();

    assert!(!format!("{config:?}").contains("secret"));
}

#[test]
fn each_missing_or_malformed_setting_refuses_by_name() {
    let cases = [
        (
            vec![(ENV_RUNNER_TOKEN, TOKEN)],
            "AGENTSFLEET_API_URL is not set",
        ),
        (
            vec![(ENV_API_URL, "  "), (ENV_RUNNER_TOKEN, TOKEN)],
            "AGENTSFLEET_API_URL is not set",
        ),
        (
            vec![(ENV_API_URL, "ftp://x"), (ENV_RUNNER_TOKEN, TOKEN)],
            "not an http or https address",
        ),
        (
            vec![(ENV_API_URL, "not a url"), (ENV_RUNNER_TOKEN, TOKEN)],
            "address is not usable",
        ),
        (
            vec![(ENV_API_URL, URL)],
            "AGENTSFLEET_RUNNER_TOKEN is not set",
        ),
        (
            vec![(ENV_API_URL, URL), (ENV_RUNNER_TOKEN, "agt_t_tenant")],
            "not an agt_r runner token",
        ),
    ];

    for (pairs, expected) in cases {
        let refusal = read(&pairs).unwrap_err().to_string();
        assert!(refusal.contains(expected), "{refusal}");
    }
}
