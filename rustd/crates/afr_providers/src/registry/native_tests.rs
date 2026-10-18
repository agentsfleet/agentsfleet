use super::{Registry, Wire};

#[test]
fn native_routes_are_distinct_from_the_gateway_route() -> crate::Result<()> {
    let registry = Registry::builtin()?;
    // pin test: literal is the contract
    let google = registry.route("gemini")?;
    assert_eq!(google.wire, Wire::Gemini);
    // pin test: literal is the contract
    assert_eq!(
        google.base.as_str(),
        "https://generativelanguage.googleapis.com/"
    );
    // pin test: literal is the contract
    assert_eq!(registry.route("openrouter")?.wire, Wire::Chat);
    // pin test: literal is the contract
    assert_eq!(registry.route("ollama")?.wire, Wire::Ollama);
    assert!(Wire::Gemini.carries_images());
    assert!(Wire::Ollama.carries_images());
    Ok(())
}
