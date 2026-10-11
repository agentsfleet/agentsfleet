//! The knobs a replica resolves its connection from, and the tags its roles
//! report under.
//!
//! Split from the parent for length; the key shapes stay there.

use std::path::Path;
use std::time::Duration;

use afd_core::env::MapEnv;
use afd_dragonfly::config::{CA_CERT_FILE_KNOB, DragonflyConfig, DragonflyRole};

const URL: &str = "rediss://:secret@localhost:6379";

fn env_with(pairs: &[(&str, &str)]) -> MapEnv {
    MapEnv::from_pairs(pairs.iter().copied())
}

/// Every role reads the one knob, because every role dials the one cluster.
///
/// Two names for one endpoint would buy a second way to misconfigure it and
/// nothing else, so `DRAGONFLY_URL` is the whole surface.
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

/// TLS is what the client dials, in every scheme spelling it dials it for.
#[test]
fn test_tls_is_the_scheme_not_a_guess() {
    for (url, tls) in [
        ("redis://localhost:6379", false),
        ("valkey://localhost:6379", false),
        (URL, true),
        ("valkeys://localhost:6379", true),
        ("REDISS://localhost:6379", true),
    ] {
        let env = env_with(&[("DRAGONFLY_URL", url)]);
        let config = DragonflyConfig::resolve(&env, DragonflyRole::Default).unwrap();
        assert_eq!(config.is_tls(), tls, "{url}");
    }
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

/// The certificate authority path is read from the knob the daemon documents.
#[test]
fn test_ca_cert_file_comes_from_the_documented_knob() {
    assert_eq!(CA_CERT_FILE_KNOB, "DRAGONFLY_TLS_CA_CERT_FILE");

    let with_ca = DragonflyConfig::resolve(
        &env_with(&[("DRAGONFLY_URL", URL), (CA_CERT_FILE_KNOB, " /tmp/ca.crt ")]),
        DragonflyRole::Default,
    )
    .unwrap();
    assert_eq!(with_ca.ca_cert_file(), Some(Path::new("/tmp/ca.crt")));

    for blank in [None, Some(""), Some("   ")] {
        let mut pairs = vec![("DRAGONFLY_URL", URL)];
        pairs.extend(blank.map(|path| (CA_CERT_FILE_KNOB, path)));
        let without = DragonflyConfig::resolve(&env_with(&pairs), DragonflyRole::Default).unwrap();
        assert!(
            without.ca_cert_file().is_none(),
            "{blank:?}: no knob means the system trust store, not an empty path"
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
    assert_eq!(DragonflyRole::Default.tag(), "default");
    assert_eq!(DragonflyRole::Api.tag(), "api");

    let tags: Vec<&str> = DragonflyRole::ALL.iter().map(|role| role.tag()).collect();
    let unique: std::collections::BTreeSet<&str> = tags.iter().copied().collect();
    assert_eq!(unique.len(), tags.len(), "two roles share a tag: {tags:?}");
    for tag in &tags {
        assert_eq!(*tag, tag.to_lowercase(), "tags are lower case");
    }
}
