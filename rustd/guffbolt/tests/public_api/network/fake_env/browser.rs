//! The browser tools over an environment's browser.

use super::*;

fn browser_tools(env: &dyn Env) -> Tools<ContextTool<'_>> {
    let config = ToolsConfig::new(["browser_open", "browser", "screenshot"]).expect("three names");
    Tools::builder()
        .with_env(env)
        .build(&config)
        .expect("an env is given")
}

/// An environment with no browser refuses each browser tool with a code
/// naming why; nothing is spawned and the run goes on.
#[tokio::test]
async fn the_browser_tools_refuse_in_an_environment_with_no_browser() {
    let env = FakeEnv::default();
    let tools = browser_tools(&env);

    let mut read = Vec::new();
    for name in ["browser_open", "browser", "screenshot"] {
        let output = dispatch(&tools, name, json!({"url": "https://example.com"})).await;
        assert_eq!(output.error_code, Some(code::BROWSER_UNAVAILABLE));
        read.push(output.text);
    }

    assert_eq!(
        read,
        [
            "[browser_unavailable] browser_open has no browser here",
            "[browser_unavailable] browser has no browser here",
            "[browser_unavailable] screenshot has no browser here",
        ]
    );
    assert_eq!(lock(&env.spawned).len(), 0, "nothing was spawned");
}

#[tokio::test]
async fn the_browser_tools_drive_the_environments_browser() {
    let env = FakeEnv {
        browser: Some(FakeBrowser::default()),
        ..FakeEnv::default()
    };
    let tools = browser_tools(&env);

    let opened = dispatch(
        &tools,
        "browser_open",
        json!({"url": "https://example.com"}),
    )
    .await;
    let acted = dispatch(&tools, "browser", json!({"action": "click"})).await;
    let shot = dispatch(&tools, "screenshot", json!({})).await;

    assert_eq!(
        (opened.error_code, opened.text.as_str()),
        (None, "opened Example Domain")
    );
    assert_eq!(
        (acted.error_code, acted.text.as_str()),
        (
            Some(code::BROWSER_UNAVAILABLE),
            "[browser_unavailable] the page closed"
        )
    );
    assert_eq!(
        shot.text,
        "Attached a screenshot (4 bytes, image/png) for your next step"
    );
    assert_eq!(
        shot.image,
        Some(Image {
            kind: ImageKind::Png,
            data: Bytes::from_static(b"\x89PNG")
        })
    );
    assert_eq!(
        env.browser
            .as_ref()
            .map(|browser| lock(&browser.opened).clone()),
        Some(vec![json!({"url": "https://example.com"})])
    );
}

/// A screenshot the browser cannot take is a failed call carrying no image.
#[tokio::test]
async fn a_screenshot_the_browser_cannot_take_fails_without_an_image() {
    let env = FakeEnv {
        browser: Some(FakeBrowser {
            closed: true,
            ..FakeBrowser::default()
        }),
        ..FakeEnv::default()
    };
    let tools = browser_tools(&env);

    let shot = dispatch(&tools, "screenshot", json!({})).await;

    assert_eq!(
        (shot.error_code, shot.text.as_str(), shot.image),
        (
            Some(code::BROWSER_UNAVAILABLE),
            "[browser_unavailable] no page is open",
            None
        )
    );
}
