//! The shapes both binaries have to agree on, checked without a server.
//!
//! Every key format, knob name and constant here is read or written by the Zig
//! daemon too. A drift in any of them is not a failed test in production — it
//! is two processes quietly using different keys against the same Redis, which
//! looks like lost events rather than like a bug.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use afd_core::env::MapEnv;
use afd_redis::Backoff;

/// The ceiling the exponential backoff must never exceed.
const BACKOFF_CAP: Duration = Duration::from_millis(1_000);
use afd_redis::config::{CA_CERT_FILE_KNOB, RedisConfig, RedisRole};
use afd_redis::ready::READY_INDEX_KEY;
use afd_redis::session::{SESSION_KEY_PREFIX, SESSION_TTL, session_key};
use afd_redis::streams::{FLEET_CONSUMER_GROUP, fleet_stream_key};

const URL: &str = "rediss://:secret@localhost:6379";

fn env_with(pairs: &[(&str, &str)]) -> MapEnv {
    MapEnv::from_pairs(pairs.iter().copied())
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf()
}

/// The key every fleet's events live on, and the group they are read under.
///
/// Compared against `queue/constants.zig` rather than against a literal
/// repeated here, so the assertion cannot drift with the thing it checks.
#[test]
fn test_stream_key_and_group_match_the_zig_constants() {
    assert_eq!(
        fleet_stream_key("fleet_0123"),
        "fleet:fleet_0123:events",
        "the producer and every consumer agree byte-for-byte or events vanish"
    );

    let constants =
        std::fs::read_to_string(repo_root().join("src/agentsfleetd/queue/constants.zig")).unwrap();
    assert!(
        constants.contains(r#"fleet_stream_prefix = "fleet:""#)
            && constants.contains(r#"fleet_stream_suffix = ":events""#),
        "the Zig side spells the stream key differently"
    );
    assert!(
        constants.contains(&format!(r#""{FLEET_CONSUMER_GROUP}""#)),
        "the Zig side reads under a different consumer group"
    );
    assert!(
        constants.contains(&format!(r#"ready_index_key = "{READY_INDEX_KEY}""#)),
        "the Zig side keeps its readiness index somewhere else"
    );
}

/// The session key and time-to-live, likewise single-sourced against Zig.
#[test]
fn test_session_key_and_ttl_match_the_zig_store() {
    assert_eq!(session_key("abc"), "auth:session:abc");
    assert_eq!(SESSION_TTL, Duration::from_secs(300));

    let store = std::fs::read_to_string(
        repo_root().join("src/agentsfleetd/session/session_store_redis.zig"),
    )
    .unwrap();
    assert!(
        store.contains(&format!(
            r#"SESSION_KEY_PREFIX: []const u8 = "{SESSION_KEY_PREFIX}""#
        )),
        "the Zig store uses a different key prefix"
    );
    assert!(
        store.contains(&format!(
            "SESSION_TTL_SECONDS: u32 = {}",
            SESSION_TTL.as_secs()
        )),
        "the Zig store uses a different time-to-live"
    );
}

/// This crate's script and the Zig daemon's are the same bytes.
///
/// Not the same FILE: M181 deletes `src/agentsfleetd/`, and a crate that
/// included a script from a directory scheduled for deletion would stop
/// building on cutover day. So each side owns its copy and this test is what
/// makes "two copies" safe — it compares them byte for byte, so a change to
/// either without the other fails here rather than as two binaries disagreeing
/// about whether a device-flow code was already redeemed.
///
/// When the Zig tree goes, this test goes with it, and the Rust copy is simply
/// the script.
#[test]
fn test_the_verify_script_matches_the_zig_daemons() {
    let ours = include_str!("../src/session/verify_consume.lua");
    let zig_path = repo_root().join("src/agentsfleetd/session/session_verify_consume.lua");

    let Ok(theirs) = std::fs::read_to_string(&zig_path) else {
        // The Zig tree is gone: cutover happened, and this crate's copy is now
        // the only one. Nothing to compare, and nothing wrong.
        return;
    };

    assert_eq!(
        ours, theirs,
        "the two copies of the verify-and-consume script have drifted — \
         one binary would redeem a session the other considers already consumed"
    );

    let proto = std::fs::read_to_string(
        repo_root().join("src/agentsfleetd/session/session_store_redis_proto.zig"),
    )
    .unwrap();
    assert!(
        proto.contains(r#"@embedFile("session_verify_consume.lua")"#),
        "the Zig daemon stopped embedding the script this test compares against"
    );
}

/// The two roles read the two knobs the daemon documents.
#[test]
fn test_role_url_knobs_match_the_zig_daemon() {
    assert_eq!(RedisRole::Default.url_knob(), "REDIS_URL");
    assert_eq!(RedisRole::Api.url_knob(), "REDIS_URL_API");
    assert_eq!(RedisRole::ALL.len(), 2, "Redis has no migrator role");
}

/// A role resolves from its own knob and is never handed another's.
#[test]
fn test_each_role_resolves_only_its_own_knob() {
    let env = env_with(&[("REDIS_URL", URL)]);
    RedisConfig::resolve(&env, RedisRole::Default).expect("REDIS_URL resolves");

    let error = RedisConfig::resolve(&env, RedisRole::Api).expect_err("no fallback between roles");
    assert!(error.is_config(), "got {error}");
    assert!(error.to_string().contains("REDIS_URL_API"));
}

/// Unset, blank, and not-a-Redis-URL are all refused at resolve.
#[test]
fn test_malformed_urls_are_refused() {
    for bad in ["", "   ", "http://localhost:6379", "localhost:6379"] {
        let env = env_with(&[("REDIS_URL", bad)]);
        let error =
            RedisConfig::resolve(&env, RedisRole::Default).expect_err("not a Redis URL: {bad:?}");
        assert!(error.is_config(), "{bad:?} gave {error}");
        assert_eq!(error.code().as_str(), "UZ-STARTUP-004");
    }
}

/// Both schemes are accepted, and only `rediss://` means TLS.
#[test]
fn test_tls_is_the_scheme_not_a_guess() {
    let plain = RedisConfig::resolve(
        &env_with(&[("REDIS_URL", "redis://localhost:6379")]),
        RedisRole::Default,
    )
    .unwrap();
    assert!(!plain.is_tls());

    let tls = RedisConfig::resolve(&env_with(&[("REDIS_URL", URL)]), RedisRole::Default).unwrap();
    assert!(tls.is_tls(), "rediss:// is the TLS spelling");
}

/// The request deadline defaults to the documented five seconds, and a knob
/// that cannot be read does not silently become zero.
#[test]
fn test_request_timeout_knob() {
    let default =
        RedisConfig::resolve(&env_with(&[("REDIS_URL", URL)]), RedisRole::Default).unwrap();
    assert_eq!(default.request_timeout(), Duration::from_millis(5_000));

    let tuned = RedisConfig::resolve(
        &env_with(&[("REDIS_URL", URL), ("REDIS_REQUEST_TIMEOUT_MS", " 250\n")]),
        RedisRole::Default,
    )
    .unwrap();
    assert_eq!(tuned.request_timeout(), Duration::from_millis(250));

    for bad in ["0", "banana", ""] {
        let config = RedisConfig::resolve(
            &env_with(&[("REDIS_URL", URL), ("REDIS_REQUEST_TIMEOUT_MS", bad)]),
            RedisRole::Default,
        )
        .unwrap();
        assert_eq!(
            config.request_timeout(),
            Duration::from_millis(5_000),
            "{bad:?} must fall back, not disable the deadline"
        );
    }
}

/// Connection establishment has its own whole-operation deadline, so a stuck
/// handshake cannot consume an unbounded lane while command tuning stays
/// independent.
#[test]
fn test_connect_timeout_knob() {
    let default =
        RedisConfig::resolve(&env_with(&[("REDIS_URL", URL)]), RedisRole::Default).unwrap();
    assert_eq!(default.connect_timeout(), Duration::from_millis(5_000));

    let tuned = RedisConfig::resolve(
        &env_with(&[("REDIS_URL", URL), ("REDIS_CONNECT_TIMEOUT_MS", "250")]),
        RedisRole::Default,
    )
    .unwrap();
    assert_eq!(tuned.connect_timeout(), Duration::from_millis(250));
    assert_eq!(
        tuned.request_timeout(),
        Duration::from_millis(5_000),
        "connection tuning must not shorten established commands"
    );

    for bad in ["0", "banana", ""] {
        let config = RedisConfig::resolve(
            &env_with(&[("REDIS_URL", URL), ("REDIS_CONNECT_TIMEOUT_MS", bad)]),
            RedisRole::Default,
        )
        .unwrap();
        assert_eq!(config.connect_timeout(), Duration::from_millis(5_000));
    }
}

/// The certificate authority path is read from the knob the Zig side reads.
#[test]
fn test_ca_cert_file_comes_from_the_documented_knob() {
    assert_eq!(CA_CERT_FILE_KNOB, "REDIS_TLS_CA_CERT_FILE");

    let with_ca = RedisConfig::resolve(
        &env_with(&[("REDIS_URL", URL), (CA_CERT_FILE_KNOB, "/tmp/ca.crt")]),
        RedisRole::Default,
    )
    .unwrap();
    assert_eq!(with_ca.ca_cert_file(), Some(Path::new("/tmp/ca.crt")));

    let without =
        RedisConfig::resolve(&env_with(&[("REDIS_URL", URL)]), RedisRole::Default).unwrap();
    assert!(
        without.ca_cert_file().is_none(),
        "no knob means the system trust store, not an empty path"
    );
}

/// The reconnect schedule grows, stops growing, and never waits forever.
///
/// The property that matters is the ceiling: a backoff that keeps doubling
/// turns a ten-minute Redis outage into an hour-long one, because the last
/// sleep started before Redis came back.
#[test]
fn test_backoff_grows_then_caps() {
    let backoff = Backoff::new(Duration::from_millis(100), Duration::from_millis(800));
    let delays: Vec<Duration> = (0..8).map(|attempt| backoff.delay(attempt, 0)).collect();

    assert_eq!(delays[0], Duration::from_millis(100));
    assert_eq!(delays[1], Duration::from_millis(200));
    assert_eq!(delays[2], Duration::from_millis(400));
    for delay in &delays {
        assert!(*delay <= BACKOFF_CAP, "{delay:?} past the cap");
    }
    assert_eq!(
        delays[7],
        Duration::from_millis(800),
        "the schedule must settle at its ceiling, not keep climbing"
    );
}

/// Jitter spreads the delay without letting it collapse or overshoot.
#[test]
fn test_backoff_jitter_stays_inside_its_quarter() {
    let backoff = Backoff::new(Duration::from_millis(400), Duration::from_millis(400));
    for jitter in [0, 1, 37, u64::MAX] {
        let delay = backoff.delay(3, jitter);
        assert!(
            delay >= Duration::from_millis(400) && delay < Duration::from_millis(500),
            "{jitter} produced {delay:?}"
        );
    }
}

/// Every role reports a distinct lower-case tag.
///
/// The tag is what a log line and an unreachable-datastore error name, so two
/// roles sharing one — or a tag that drifts from the knob it belongs to —
/// sends whoever is reading the incident at the wrong connection.
#[test]
fn test_every_role_tags_itself_distinctly() {
    assert_eq!(RedisRole::Default.tag(), "default");
    assert_eq!(RedisRole::Api.tag(), "api");

    let tags: Vec<&str> = RedisRole::ALL.iter().map(|role| role.tag()).collect();
    let unique: std::collections::BTreeSet<&str> = tags.iter().copied().collect();
    assert_eq!(unique.len(), tags.len(), "two roles share a tag: {tags:?}");
    for tag in &tags {
        assert_eq!(*tag, tag.to_lowercase(), "tags are lower case");
    }
}

/// Every reply shape a stream field can carry renders as readable text.
///
/// The producer writes bulk strings, so the other arms only run when something
/// upstream changed. Rendering one as an empty string would hand a caller a
/// field that looks absent rather than surprising.
#[test]
fn test_every_stream_field_reply_shape_renders() {
    let samples = afd_redis::streams::rendered_field_samples();
    assert_eq!(samples.len(), 4, "a reply shape was added without a sample");

    for (label, rendered) in &samples {
        assert!(!rendered.is_empty(), "{label} rendered as nothing");
    }
    let by_label = |wanted: &str| {
        samples
            .iter()
            .find(|(label, _)| *label == wanted)
            .map(|(_, rendered)| rendered.as_str())
            .expect("sample present")
    };
    assert_eq!(by_label("bulk string"), "ready");
    assert_eq!(by_label("simple string"), "OK");
    assert_eq!(by_label("integer"), "42");
    assert_eq!(
        by_label("anything else"),
        "nil",
        "an unrecognised value keeps its shape visible through Debug"
    );
}
