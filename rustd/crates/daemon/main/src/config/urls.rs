//! Public deployment files carry HTTP locations without embedded credentials.

pub(super) fn public(raw: &str) -> garde::Result {
    let url = url::Url::parse(raw)
        .map_err(|_failure| garde::Error::new("requires an absolute HTTP or HTTPS URL"))?;
    if matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
    {
        Ok(())
    } else {
        Err(garde::Error::new(
            "requires an HTTP or HTTPS URL without embedded credentials",
        ))
    }
}
