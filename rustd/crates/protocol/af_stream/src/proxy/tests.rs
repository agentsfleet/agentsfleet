//! Route policy is tested at the parsed origin, before any native socket can bypass it.

use af_core::env::MapEnv;

use crate::{Endpoint, Reason, Result};

#[test]
fn unused_proxy_bypass_settings_do_not_change_direct_routing() -> Result<()> {
    let endpoint = Endpoint::parse("https://daemon.internal:4443")?
        .with_environment_route(&MapEnv::from_pairs([("NO_PROXY", "unused/invalid")]))?;
    assert!(endpoint.proxy.is_none());
    Ok(())
}

#[test]
fn an_explicit_proxy_is_retained_and_credentials_stay_redacted() -> Result<()> {
    let env = MapEnv::from_pairs([(
        "HTTPS_PROXY",
        "http://fixture-user:fixture-value@proxy.test:8080",
    )]);
    let endpoint = Endpoint::parse("https://daemon.internal:4443")?.with_environment_route(&env)?;
    assert!(endpoint.proxy.is_some());
    let rendered = format!("{endpoint:?}");
    assert!(!rendered.contains("fixture-user"));
    assert!(!rendered.contains("fixture-value"));
    assert!(!rendered.contains("Zml4dHVyZS11c2Vy"));
    Ok(())
}

#[test]
fn standard_bypasses_match_hosts_suffixes_and_the_global_wildcard() -> Result<()> {
    for (bypass, direct) in [
        ("internal", true),
        (".internal", true),
        ("*.internal", false),
        ("daemon.internal:4443", false),
        ("daemon.internal:443", false),
        ("daemon.internal.evil", false),
        ("*", true),
    ] {
        let env = MapEnv::from_pairs([
            ("HTTPS_PROXY", "http://proxy.test:8080"),
            ("NO_PROXY", bypass),
        ]);
        let endpoint =
            Endpoint::parse("https://daemon.internal:4443")?.with_environment_route(&env)?;
        assert_eq!(endpoint.proxy.is_none(), direct, "{bypass}");
    }
    Ok(())
}

#[test]
fn bypass_networks_cover_both_address_families_without_resolving_names() -> Result<()> {
    for (origin, network, direct) in [
        ("https://127.0.0.1:4443", "127.0.0.0/8", true),
        ("https://127.0.0.1:4443", "10.0.0.0/8", false),
        ("https://[::1]:4443", "::1/128", true),
        ("https://[::1]:4443", "fc00::/7", false),
        ("https://daemon.internal:4443", "0.0.0.0/0", false),
    ] {
        let env = MapEnv::from_pairs([
            ("HTTPS_PROXY", "http://proxy.test:8080"),
            ("NO_PROXY", network),
        ]);
        let endpoint = Endpoint::parse(origin)?.with_environment_route(&env)?;
        assert_eq!(endpoint.proxy.is_none(), direct, "{network}");
    }
    Ok(())
}

#[test]
fn invalid_or_unsupported_proxy_policy_fails_closed() -> Result<()> {
    for (proxy, bypass) in [
        ("socks5://proxy.test", ""),
        ("https://proxy.test", ""),
        ("http://proxy.test/path", ""),
        ("invalid URL", "*"),
    ] {
        let env = MapEnv::from_pairs([("HTTPS_PROXY", proxy), ("NO_PROXY", bypass)]);
        let result = Endpoint::parse("https://daemon.internal:4443")?.with_environment_route(&env);
        assert!(result.is_err_and(|error| error.reason() == Reason::Protocol));
    }
    Ok(())
}

#[test]
fn unrecognized_bypass_entries_keep_the_explicit_proxy() -> Result<()> {
    for bypass in [
        "127.0.0.0/33",
        "::/129",
        "*daemon.internal",
        "daemon.internal:4443",
    ] {
        let env = MapEnv::from_pairs([
            ("HTTPS_PROXY", "http://proxy.test:8080"),
            ("NO_PROXY", bypass),
        ]);
        let endpoint =
            Endpoint::parse("https://daemon.internal:4443")?.with_environment_route(&env)?;
        assert!(endpoint.proxy.is_some(), "{bypass}");
    }
    Ok(())
}

#[tokio::test]
async fn a_transient_connect_refusal_can_retry_but_proxy_authentication_is_terminal() -> Result<()>
{
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let proxy = format!("http://{}", listener.local_addr()?);
    let env = MapEnv::from_pairs([("HTTPS_PROXY", proxy.as_str())]);
    let endpoint = Endpoint::parse("https://daemon.internal:4443")?.with_environment_route(&env)?;
    let server = tokio::spawn(async move {
        for status in [503, 200, 407] {
            let (mut stream, _) = listener.accept().await?;
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                header.push(stream.read_u8().await?);
                assert!(header.len() <= 4096);
            }
            assert!(header.starts_with(b"CONNECT daemon.internal:4443 HTTP/1.1"));
            stream
                .write_all(
                    format!("HTTP/1.1 {status} Fixture\r\nContent-Length: 0\r\n\r\n").as_bytes(),
                )
                .await?;
            if status == 200 {
                assert_eq!(stream.read(&mut [0; 1]).await?, 0);
            }
        }
        std::io::Result::Ok(())
    });
    let transient = super::socket(&endpoint)
        .await
        .expect_err("503 refuses this attempt");
    assert_eq!(transient.reason(), Reason::Network);
    assert!(transient.to_string().contains("503"));
    drop(super::socket(&endpoint).await?);
    let terminal = super::socket(&endpoint)
        .await
        .expect_err("407 requires operator action");
    assert_eq!(terminal.reason(), Reason::Protocol);
    assert!(terminal.to_string().contains("407"));
    tokio::time::timeout(std::time::Duration::from_secs(5), server).await???;
    Ok(())
}
