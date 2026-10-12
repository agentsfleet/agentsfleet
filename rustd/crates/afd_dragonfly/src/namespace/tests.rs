//! Configuration failures are reported before any transport is opened.

#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use crate::{DragonflyConfig, DragonflyRole, NAMESPACE_KNOB};
use afd_core::env::MapEnv;

fn config(raw: &str) -> crate::error::Result<DragonflyConfig> {
    DragonflyConfig::resolve(
        &MapEnv::from_pairs([
            (crate::config::URL_KNOB, "redis://127.0.0.1:1"),
            (NAMESPACE_KNOB, raw),
        ]),
        DragonflyRole::Api,
    )
}

#[test]
fn namespace_config_refuses_unsafe_input() {
    for raw in ["a:b", "*", "?", "[ab]", "a{b}", "a\\b", "a b", "a\nb"] {
        let error = config(raw).expect_err("unsafe namespace refuses without a socket");
        assert!(error.is_config());
        assert_eq!(error.code().as_str(), "UZ-STARTUP-004");
        assert!(error.to_string().contains(NAMESPACE_KNOB));
        assert!(std::error::Error::source(&error).is_some());
    }
}

#[test]
fn namespace_config_refuses_oversized_input() {
    config(&"a".repeat(64)).expect("maximum namespace length is accepted");
    assert!(
        config(&"a".repeat(65))
            .expect_err("length limit")
            .is_config()
    );
}

#[test]
fn namespace_config_refuses_unicode_and_surrounding_whitespace() {
    for raw in ["こんにちは", "foo ", " foo", "foo\0bar"] {
        assert!(config(raw).expect_err("unsafe namespace").is_config());
    }
}

#[test]
fn namespace_builder_enforces_the_same_bounds_as_environment_resolution() {
    let original = DragonflyConfig::from_url(DragonflyRole::Default, "redis://127.0.0.1:1".into());
    original
        .clone()
        .with_namespace("wt_abc-123")
        .expect("safe namespace");
    assert!(
        original
            .with_namespace("*")
            .expect_err("builder validates")
            .is_config()
    );
}

#[test]
fn namespace_config_accepts_unset_blank_and_safe_values_for_every_role() {
    for role in DragonflyRole::ALL {
        let env = MapEnv::from_pairs([(crate::config::URL_KNOB, "redis://127.0.0.1:1")]);
        DragonflyConfig::resolve(&env, *role).expect("unset namespace");
    }
    for raw in ["", " \n", "wt_abc-123"] {
        config(raw).expect("blank or safe namespace");
    }
}

#[test]
fn namespace_default_preserves_every_logical_name() {
    for raw in ["", " \n"] {
        let namespace = config(raw).expect("default configuration").namespace;
        for logical in ["key", "fleet:{tag}:events", "dev:other:key"] {
            assert_eq!(namespace.key(logical), logical);
            assert_eq!(namespace.logical(logical), Some(logical));
        }
    }
}

#[test]
fn namespace_round_trip_rejects_foreign_prefixes() {
    let namespace = config("x").expect("partitioned configuration").namespace;
    assert_eq!(namespace.key("key"), "dev:x:key");
    assert_eq!(namespace.logical("dev:x:key"), Some("key"));
    for foreign in ["dev:xy:key", "dev:y:key", "key", "dev:x"] {
        assert_eq!(namespace.logical(foreign), None, "{foreign}");
    }
}

#[test]
fn namespace_preserves_hash_tags_and_namespace_like_logical_names() {
    let namespace = config("x").expect("partitioned configuration").namespace;
    let tagged = "fleet:{tag}:events";
    assert_eq!(
        redis::cluster_routing::Slot::for_key(tagged),
        redis::cluster_routing::Slot::for_key(namespace.key(tagged))
    );
    assert_ne!(
        redis::cluster_routing::Slot::for_key("key"),
        redis::cluster_routing::Slot::for_key(namespace.key("key"))
    );
    for logical in ["fleet:{tag}:events", "dev:y:key", "STREAMS", "dev:x:key"] {
        let physical = namespace.key(logical);
        assert_eq!(physical, format!("dev:x:{logical}"));
        assert_eq!(namespace.logical(&physical), Some(logical));
    }
}
