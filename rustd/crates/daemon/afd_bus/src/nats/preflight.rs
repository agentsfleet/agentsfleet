//! Boot refuses a server this daemon cannot rely on, before serving anything.

use async_nats::ServerInfo;

use crate::{Result, error::ErrorKind};

/// The tail's largest batch: a final reply's inline text, 64 KiB
/// (`afd_fleet::lease::bracket::MAX_INLINE_FINAL_REPLY_BYTES`).
const TAIL_BATCH_BYTES: usize = 64 * 1024;
/// The draft stream's largest frame, 32 KiB
/// (`afd_api_tenant::handler::draft::stream::body::MAX_FRAME_BYTES`).
const DRAFT_FRAME_BYTES: usize = 32 * 1024;
/// Room for the JSON envelope around the largest frame.
const ENVELOPE_BYTES: usize = 64 * 1024;

/// The smallest `max_payload` every publisher's frame fits under.
pub(super) const LARGEST_FRAME_BYTES: usize = TAIL_BATCH_BYTES + ENVELOPE_BYTES;

const _: () = assert!(LARGEST_FRAME_BYTES >= TAIL_BATCH_BYTES);
const _: () = assert!(LARGEST_FRAME_BYTES >= DRAFT_FRAME_BYTES);

/// Refuse `JetStream`, a lone server, a server without headers, or a payload
/// limit under the largest frame. Each names the field that failed.
///
/// # Errors
/// `UZ-STARTUP-004` naming `jetstream`, `cluster`, `headers` or `max_payload`.
pub(super) fn check(info: &ServerInfo) -> Result<()> {
    // JetStream on means NATS could hold state; Postgres owns every fact.
    let field = if info.jetstream {
        Some("jetstream")
    } else if info.cluster.is_none() {
        // A lone server is a single point of loss; the drill assumes peers.
        Some("cluster")
    } else if !info.headers {
        // The gap marker is a header.
        Some("headers")
    } else if info.max_payload < LARGEST_FRAME_BYTES {
        Some("max_payload")
    } else {
        None
    };
    field.map_or(Ok(()), |field| {
        Err(ErrorKind::NatsUnsuitable { field }.into())
    })
}

#[cfg(test)]
mod tests {
    use af_core::error_code;
    use async_nats::ServerInfo;

    use super::{LARGEST_FRAME_BYTES, check};

    type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

    fn suitable() -> ServerInfo {
        ServerInfo {
            cluster: Some("agentsfleet".to_owned()),
            headers: true,
            max_payload: 1024 * 1024,
            ..ServerInfo::default()
        }
    }

    #[test]
    fn a_core_cluster_with_headers_and_room_boots() -> TestResult {
        check(&suitable())?;
        check(&ServerInfo {
            max_payload: LARGEST_FRAME_BYTES,
            ..suitable()
        })?;
        Ok(())
    }

    #[test]
    fn boot_refuses_an_unsuitable_nats_server() -> TestResult {
        let cases = [
            (
                ServerInfo {
                    jetstream: true,
                    ..suitable()
                },
                "jetstream",
            ),
            (
                ServerInfo {
                    cluster: None,
                    ..suitable()
                },
                "cluster",
            ),
            (
                ServerInfo {
                    headers: false,
                    ..suitable()
                },
                "headers",
            ),
            (
                ServerInfo {
                    max_payload: 32_768,
                    ..suitable()
                },
                "max_payload",
            ),
        ];
        for (info, field) in cases {
            let error = check(&info).err().ok_or(field)?;
            assert_eq!(error.code(), error_code::STARTUP_DRAGONFLY_CONNECT);
            assert_eq!(error.reason(), "backend");
            assert!(error.to_string().contains(field), "{error}");
            assert!(error.to_string().contains("NATS_URL"), "{error}");
        }
        Ok(())
    }
}
