//! The shapes both binaries have to agree on, checked without a server.
//!
//! Every key format, knob name and constant here is read or written by the Zig
//! daemon too. A drift in any of them is not a failed test in production — it
//! is two processes quietly using different keys against the same Dragonfly, which
//! looks like lost events rather than like a bug.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::path::Path;
use std::time::Duration;

use afd_core::env::MapEnv;
use afd_dragonfly::production_backoff;
use backon::BackoffBuilder as _;

/// The ceiling the reconnect schedule must never exceed.
///
/// Five seconds, the value `production_backoff` sets. Spelled here as the
/// PROMISE rather than read from the source, so a change to the schedule has to
/// change this line too and be seen: how long an outage takes to recover from
/// is an operational number, not an implementation detail.
const BACKOFF_CAP: Duration = Duration::from_secs(5);
use afd_dragonfly::config::{CA_CERT_FILE_KNOB, DragonflyConfig, DragonflyRole};
use afd_dragonfly::ready::{Partition, READY_INDEX_KEY, READY_PARTITIONS};
use afd_dragonfly::session::{SESSION_KEY_PREFIX, SESSION_TTL, session_key};
use afd_dragonfly::streams::{FLEET_CONSUMER_GROUP, fleet_stream_key};

const URL: &str = "rediss://:secret@localhost:6379";

fn env_with(pairs: &[(&str, &str)]) -> MapEnv {
    MapEnv::from_pairs(pairs.iter().copied())
}

/// The key every fleet's events live on, and the group they are read under.
///
/// The three names a Dragonfly key is built from, frozen as `queue/constants.zig`
/// spelled them at sunset.
///
/// These were read out of that file at test time so the assertion could not
/// drift with the thing it checked. The tree is deleted in this milestone, so
/// the values are pinned here instead. They stay worth asserting for the reason
/// they always were, which was never about Zig: a producer and its consumers
/// agree on these bytes or events vanish silently, and a key renamed in a
/// refactor strands every entry already written under the old one.
#[test]
fn test_stream_key_and_group_match_the_zig_constants() {
    assert_eq!(
        fleet_stream_key("fleet_0123"),
        "fleet:fleet_0123:events",
        "the producer and every consumer agree byte-for-byte or events vanish"
    );
    assert_eq!(
        FLEET_CONSUMER_GROUP, "fleet_lease",
        "every consumer reads under this group or a rename strands the backlog"
    );
    assert_eq!(
        READY_INDEX_KEY, "fleet:ready",
        "the readiness index moves and every entry already written is orphaned"
    );
    // pin test: literal is the contract
    assert_eq!(READY_PARTITIONS, 16, "a changed count re-homes every mark");
    assert_eq!(
        Partition::of("fleet_0123").key(),
        format!("fleet:ready:{{{}}}", Partition::of("fleet_0123").index()),
        "the partition key is the index key, a colon, and the number as a hash tag"
    );
}

/// The session key and time-to-live, frozen as `session_store_redis.zig`
/// declared them at sunset.
///
/// Read from that file until the tree's deletion; pinned here now. The property
/// outlives its source: a prefix or a lifetime that moves without a migration
/// signs every live session out at once.
#[test]
fn test_session_key_and_ttl_match_the_zig_store() {
    assert_eq!(session_key("abc"), "auth:session:abc");
    assert_eq!(SESSION_TTL, Duration::from_secs(300));

    assert_eq!(
        SESSION_KEY_PREFIX, "auth:session:",
        "a moved prefix signs every live session out at once"
    );
}

/// Every role reads the one knob, because every role dials the one cluster.
///
/// The pair used to be `REDIS_URL` and `REDIS_URL_API`, spelled to match the
/// retired Zig daemon so a deployment could move between binaries without
/// touching its environment. Two names for one endpoint bought a second way to
/// misconfigure it, and the binary that justified them is gone.
#[test]
fn test_every_role_reads_the_one_url_knob() {
    for role in DragonflyRole::ALL {
        assert_eq!(role.url_knob(), afd_dragonfly::config::URL_KNOB);
    }
    assert_eq!(
        DragonflyRole::ALL.len(),
        2,
        "Dragonfly has no migrator role"
    );
}

/// One knob serves every role, and its absence refuses every role.
///
/// The inverse of the old test, which proved a role could not read another's
/// knob. There is no other knob now, so what is worth pinning is that the
/// collapse did not quietly give one role a fallback the other lacks.
#[test]
fn test_the_one_knob_serves_and_refuses_every_role() {
    let env = env_with(&[("DRAGONFLY_URL", URL)]);
    for role in DragonflyRole::ALL {
        DragonflyConfig::resolve(&env, *role).expect("the one knob resolves for every role");
    }

    let empty = env_with(&[]);
    for role in DragonflyRole::ALL {
        let error = DragonflyConfig::resolve(&empty, *role).expect_err("an unset knob refuses");
        assert!(error.is_config(), "got {error}");
        assert!(error.to_string().contains("DRAGONFLY_URL"));
    }
}

/// Unset, blank, and not-a-Dragonfly-URL are all refused at resolve.
///
/// The last two cases are the ones a scheme-prefix check let through: each
/// starts with the seven characters that check looked for and neither is a URL,
/// so they used to reach `Client::open` and come back as UNREACHABLE — a
/// network-shaped error for an environment-shaped fault. Validating through the
/// client's own parser is what moved them here, where the message names the
/// knob to go and fix.
///
/// Not every typo is reachable this way, and the ones that are not are left
/// out rather than asserted loosely: `url` accepts invalid percent-encoding in
/// userinfo, so `redis://%zz@host` parses and is still a connect-time failure.
/// A case list that claimed otherwise would be documenting a guarantee this
/// check does not make.
#[test]
fn test_malformed_urls_are_refused() {
    for bad in [
        "",
        "   ",
        "http://localhost:6379",
        "localhost:6379",
        "redis://[::1",
        "redis://localhost:not-a-port",
    ] {
        let env = env_with(&[("DRAGONFLY_URL", bad)]);
        let error = DragonflyConfig::resolve(&env, DragonflyRole::Default)
            .expect_err("not a Dragonfly URL: {bad:?}");
        assert!(error.is_config(), "{bad:?} gave {error}");
        assert_eq!(error.code().as_str(), "UZ-STARTUP-004");
    }
}

/// Both schemes are accepted, and only `rediss://` means TLS.
#[test]
fn test_tls_is_the_scheme_not_a_guess() {
    let plain = DragonflyConfig::resolve(
        &env_with(&[("DRAGONFLY_URL", "redis://localhost:6379")]),
        DragonflyRole::Default,
    )
    .unwrap();
    assert!(!plain.is_tls());

    let tls =
        DragonflyConfig::resolve(&env_with(&[("DRAGONFLY_URL", URL)]), DragonflyRole::Default)
            .unwrap();
    assert!(tls.is_tls(), "rediss:// is the TLS spelling");
}

/// The request deadline defaults to the documented five seconds, and a knob
/// that cannot be read does not silently become zero.
#[test]
fn test_request_timeout_knob() {
    let default =
        DragonflyConfig::resolve(&env_with(&[("DRAGONFLY_URL", URL)]), DragonflyRole::Default)
            .unwrap();
    assert_eq!(default.request_timeout(), Duration::from_millis(5_000));

    let tuned = DragonflyConfig::resolve(
        &env_with(&[
            ("DRAGONFLY_URL", URL),
            ("DRAGONFLY_REQUEST_TIMEOUT_MS", " 250\n"),
        ]),
        DragonflyRole::Default,
    )
    .unwrap();
    assert_eq!(tuned.request_timeout(), Duration::from_millis(250));

    for bad in ["0", "banana", ""] {
        let config = DragonflyConfig::resolve(
            &env_with(&[
                ("DRAGONFLY_URL", URL),
                ("DRAGONFLY_REQUEST_TIMEOUT_MS", bad),
            ]),
            DragonflyRole::Default,
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
        DragonflyConfig::resolve(&env_with(&[("DRAGONFLY_URL", URL)]), DragonflyRole::Default)
            .unwrap();
    assert_eq!(default.connect_timeout(), Duration::from_millis(5_000));

    let tuned = DragonflyConfig::resolve(
        &env_with(&[
            ("DRAGONFLY_URL", URL),
            ("DRAGONFLY_CONNECT_TIMEOUT_MS", "250"),
        ]),
        DragonflyRole::Default,
    )
    .unwrap();
    assert_eq!(tuned.connect_timeout(), Duration::from_millis(250));
    assert_eq!(
        tuned.request_timeout(),
        Duration::from_millis(5_000),
        "connection tuning must not shorten established commands"
    );

    for bad in ["0", "banana", ""] {
        let config = DragonflyConfig::resolve(
            &env_with(&[
                ("DRAGONFLY_URL", URL),
                ("DRAGONFLY_CONNECT_TIMEOUT_MS", bad),
            ]),
            DragonflyRole::Default,
        )
        .unwrap();
        assert_eq!(config.connect_timeout(), Duration::from_millis(5_000));
    }
}

/// The certificate authority path is read from the knob the Zig side reads.
#[test]
fn test_ca_cert_file_comes_from_the_documented_knob() {
    assert_eq!(CA_CERT_FILE_KNOB, "DRAGONFLY_TLS_CA_CERT_FILE");

    let with_ca = DragonflyConfig::resolve(
        &env_with(&[("DRAGONFLY_URL", URL), (CA_CERT_FILE_KNOB, "/tmp/ca.crt")]),
        DragonflyRole::Default,
    )
    .unwrap();
    assert_eq!(with_ca.ca_cert_file(), Some(Path::new("/tmp/ca.crt")));

    let without =
        DragonflyConfig::resolve(&env_with(&[("DRAGONFLY_URL", URL)]), DragonflyRole::Default)
            .unwrap();
    assert!(
        without.ca_cert_file().is_none(),
        "no knob means the system trust store, not an empty path"
    );
}

/// The reconnect schedule grows, stops growing, and never waits forever.
///
/// `backon` owns the arithmetic, so what is asserted here is our CONFIGURATION
/// of it — the two knobs an operator feels. The ceiling is the one that matters:
/// a backoff that keeps doubling turns a ten-minute Dragonfly outage into an
/// hour-long one, because the last sleep started before Dragonfly came back.
///
/// Jitter is on in production, so each delay is a random offset inside its
/// step rather than a fixed number. The assertions are therefore bounds, which
/// is what the property actually is — an equality here would be asserting that
/// the spread is absent.
#[test]
fn test_the_reconnect_schedule_grows_then_settles_at_its_ceiling() {
    let delays: Vec<Duration> = production_backoff().build().take(12).collect();

    // Jitter ADDS inside the step, so the first delay lands in [200, 400) —
    // asserting equality with the floor would be asserting the spread away.
    assert!(
        delays[0] >= Duration::from_millis(200) && delays[0] < Duration::from_millis(400),
        "the first redial waits one step plus its spread: {:?}",
        delays[0]
    );
    for delay in &delays {
        // The cap bounds the STEP and the jitter is added to it, so the wait
        // itself tops out at twice the ceiling.
        assert!(*delay < BACKOFF_CAP * 2, "{delay:?} past the cap");
    }
    let settled = delays
        .last()
        .copied()
        .expect("a schedule with no attempt limit yields twelve delays");
    assert!(
        settled > Duration::from_secs(1),
        "the schedule must climb to its ceiling rather than staying at the \
         floor: {settled:?}"
    );
}

/// Jitter spreads the delay, so two processes that lost the same Dragonfly do not
/// redial in the same millisecond.
///
/// The reconnect storm is what keeps a struggling Dragonfly down, and a schedule
/// that produced one sequence for everybody would cause it.
#[test]
fn test_the_reconnect_schedule_is_spread() {
    let one: Vec<Duration> = production_backoff().build().take(8).collect();
    let other: Vec<Duration> = production_backoff().build().take(8).collect();

    assert_ne!(one, other);
}

/// Every role reports a distinct lower-case tag.
///
/// The tag is what a log line and an unreachable-datastore error name, so two
/// roles sharing one — or a tag that drifts from the knob it belongs to —
/// sends whoever is reading the incident at the wrong connection.
#[test]
fn test_every_role_tags_itself_distinctly() {
    assert_eq!(DragonflyRole::Default.tag(), "default");
    assert_eq!(DragonflyRole::Api.tag(), "api");

    let tags: Vec<&str> = DragonflyRole::ALL.iter().map(|role| role.tag()).collect();
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
    let samples = afd_dragonfly::streams::rendered_field_samples();
    assert_eq!(samples.len(), 5, "a reply shape was added without a sample");

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
    assert_eq!(
        by_label("invalid utf-8"),
        "binary-data([255, 254])",
        "a field this daemon did not write shows its bytes rather than a \
         sentence with replacement characters punched through it"
    );
}

/// Every abort reason spells itself the way the stored value reads.
///
/// The reason rides `session:*` in Dragonfly and BOTH binaries read it back. A
/// respelling here is not a failed test in production — it is one process
/// writing a reason the other cannot classify, on a record that exists to say
/// why somebody's run was stopped.
#[test]
fn every_abort_reason_carries_its_stored_spelling() {
    use afd_dragonfly::session::AbortReason;

    assert_eq!(AbortReason::ExplicitCancel.as_str(), "explicit_cancel");
    assert_eq!(
        AbortReason::RateLimitExceeded.as_str(),
        "rate_limit_exceeded"
    );
    assert_ne!(
        AbortReason::ExplicitCancel.as_str(),
        AbortReason::RateLimitExceeded.as_str(),
        "a person cancelling and a ceiling stopping them are different facts"
    );
}
