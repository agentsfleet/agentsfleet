//! A collection finds a provider by id and a model by its catalogue row,
//! reads the provider's key from its variable, and refuses what it cannot
//! find before anything is sent.

#![expect(clippy::unwrap_used, reason = "fixture preconditions must fail loudly")]

use std::collections::HashSet;

use super::Models;
#[cfg(feature = "minimax")]
use super::Spec;
#[cfg(feature = "minimax")]
use crate::Model as _;
#[cfg(feature = "minimax")]
use crate::providers;

#[cfg(feature = "minimax")]
const FLASH: &str = "MiniMax-M3.1-Flash";

// Value: every compiled-in row is one a route can be built from, and a
// provider never names one model twice.
#[test]
fn every_catalogue_row_parses_and_fits() {
    let models = Models::builtin();
    for provider in &models.providers {
        let rows = provider.specs().unwrap();
        let mut ids = HashSet::new();
        for row in &rows {
            assert!(
                row.max_output_tokens <= row.context_tokens,
                "{} {}",
                provider.id(),
                row.id
            );
            assert!(row.max_output_tokens > 0, "{} {}", provider.id(), row.id);
            assert!(ids.insert(row.id.as_str()), "{} {}", provider.id(), row.id);
        }
    }
}

// Value: the error names the variable to set and carries nothing else.
#[cfg(feature = "minimax")]
#[test]
fn connect_without_the_key_names_its_variable() {
    let models = Models::new().with(providers::minimax());

    for unset in [None, Some(String::new())] {
        let refused = models
            .connect_reading("minimax", FLASH, |_| unset.clone())
            .unwrap_err();

        assert_eq!(
            refused.to_string(),
            "the provider's key is missing: set MINIMAX_API_KEY"
        );
        assert!(refused.is_invalid_request());
    }
}

// Value: a model the catalogue lacks is refused by name, whatever the
// environment's key, before any request.
#[cfg(feature = "minimax")]
#[test]
fn an_unknown_model_id_is_refused_with_its_provider() {
    let models = Models::new().with(providers::minimax());

    let refused = models.connect("minimax", "no-such-model").unwrap_err();

    assert_eq!(
        refused.to_string(),
        "provider minimax has no model no-such-model"
    );
    assert!(refused.is_invalid_request());
}

// Value: a provider whose feature is off, or that was never added, is named
// in the refusal.
#[cfg(feature = "minimax")]
#[test]
fn an_unregistered_provider_is_refused() {
    let refused = Models::new().connect("minimax", FLASH).unwrap_err();

    assert_eq!(refused.to_string(), "provider minimax is not registered");
    assert!(refused.is_invalid_request());
    let missing = Models::new().get("minimax", FLASH).unwrap_err();
    assert_eq!(missing.to_string(), "provider minimax is not registered");
}

// Value: a model offers images only when its row reads them and its wire
// carries them.
#[cfg(feature = "minimax")]
#[test]
fn accepts_images_follows_the_row_and_the_wire() {
    let models = Models::new().with(providers::minimax());
    let messages = models.connect_with_key("minimax", FLASH, "k").unwrap();
    let route = crate::wire::Route::new(
        crate::wire::Api::Chat,
        "https://chat.example/v1",
        None,
        1,
        2,
    )
    .unwrap();
    let chat = crate::wire::Connected::new(
        route,
        FLASH,
        "k",
        crate::wire::transport::Reqwest::new().unwrap(),
    );
    let text_only = Models::new()
        .with(providers::minimax().with_spec(Spec {
            images: false,
            ..models.get("minimax", FLASH).unwrap()
        }))
        .connect_with_key("minimax", FLASH, "k")
        .unwrap();

    assert!(messages.accepts_images());
    assert!(!chat.accepts_images());
    assert!(!text_only.accepts_images());
}

// Value: a connected model is bounded by its row's numbers, on the
// provider's wire and base URL.
#[cfg(feature = "minimax")]
#[test]
fn a_connected_model_takes_its_row_and_its_provider() {
    let models = Models::new().with(providers::minimax());

    let model = models.connect_with_key("minimax", FLASH, "k").unwrap();

    let route = model.route();
    assert_eq!(route.api(), crate::wire::Api::Messages);
    assert_eq!(route.base_url(), "https://api.minimax.io/anthropic");
    assert_eq!(route.context_tokens(), 1_000_000);
    assert_eq!(route.max_output_tokens(), 524_288);
    assert_eq!(providers::minimax().key_variable(), "MINIMAX_API_KEY");
}

// Value: a provider added again replaces the first, so a base URL set later
// is the one requests go to.
#[cfg(feature = "minimax")]
#[test]
fn a_provider_added_twice_is_the_later_one() {
    let proxied = providers::minimax().with_base_url("https://proxy.example/anthropic");
    assert_eq!(
        providers::minimax().base_url(),
        "https://api.minimax.io/anthropic"
    );
    assert_eq!(proxied.base_url(), "https://proxy.example/anthropic");
    let models = Models::new().with(providers::minimax()).with(proxied);

    let model = models.connect_with_key("minimax", FLASH, "k").unwrap();

    assert_eq!(model.route().base_url(), "https://proxy.example/anthropic");
    assert_eq!(models.providers.len(), 1);
}

// Value: a row added for a model the catalogue lacks, or in place of one it
// holds, is the row a connect reads; one past its window is refused.
#[cfg(feature = "minimax")]
#[test]
fn an_added_row_is_served_and_still_checked() {
    let added = Spec {
        id: "MiniMax-Next".into(),
        context_tokens: 4096,
        max_output_tokens: 1024,
        images: false,
    };
    let oversized = Spec {
        id: FLASH.into(),
        context_tokens: 10,
        max_output_tokens: 20,
        images: true,
    };
    let models = Models::new().with(
        providers::minimax()
            .with_spec(added.clone())
            .with_spec(added.clone())
            .with_spec(oversized.clone()),
    );

    assert_eq!(models.get("minimax", "MiniMax-Next").unwrap(), added);
    assert_eq!(models.get("minimax", FLASH).unwrap(), oversized);
    let refused = models.connect_with_key("minimax", FLASH, "k").unwrap_err();
    assert!(refused.is_invalid_request(), "{refused:?}");
}

// Value: a catalogue that is not the rows it should hold names its
// provider rather than serving none.
#[test]
fn an_unreadable_catalogue_names_its_provider() {
    let provider = super::Provider::new(
        "broken",
        crate::wire::Api::Chat,
        None,
        "https://broken.example/v1",
        ("BROKEN_API_KEY", true),
        "[{\"id\": 1}]",
    );

    let refused = Models::new()
        .with(provider)
        .get("broken", "any")
        .unwrap_err();

    assert_eq!(
        refused.to_string(),
        "the model catalogue of provider broken cannot be read"
    );
}

// Value: a local server's key is optional: unset, the model still connects,
// read through the process environment.
#[cfg(feature = "ollama")]
#[test]
fn an_optional_key_unset_still_connects() {
    let models = Models::new().with(providers::ollama());
    let id = providers::ollama().specs().unwrap().remove(0).id;

    let unkeyed = models.connect_reading("ollama", &id, |_| None).unwrap();
    let read = models.connect("ollama", &id).unwrap();

    assert_eq!(unkeyed.route().base_url(), "http://localhost:11434");
    assert_eq!(read.route().base_url(), "http://localhost:11434");
}
