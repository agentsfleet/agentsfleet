use super::*;

/// The consumer name is what a restart comes back to, so it must carry
/// nothing that differs between two runs of the same instance.
///
/// Asserted as an EQUALITY against the two inputs the name is built from,
/// not as "it does not contain a process id": the failure being guarded
/// against is a name gaining a per-run component, and only a full-string
/// comparison catches every shape of that.
#[test]
fn test_the_consumer_name_is_the_prefix_and_the_host_and_nothing_else() {
    let host = hostname::get()
        .ok()
        .and_then(|name| name.into_string().ok())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| CONSUMER_FALLBACK_HOST.to_owned());

    assert_eq!(
        outbound_consumer(),
        format!("{CONSUMER_PREFIX}-{host}"),
        "a name with any per-run component would strand every pending \
         entry the previous process was handed"
    );
    assert_eq!(
        outbound_consumer(),
        outbound_consumer(),
        "two calls in one process must agree, which a clock or a counter \
         in the name would break first"
    );
}
