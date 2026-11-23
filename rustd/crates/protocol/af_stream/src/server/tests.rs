//! `bind_pair` stages the UDP boundary, so a taken port is deterministic instead of a race.

use std::{
    io,
    net::{Ipv4Addr, SocketAddr},
};

use tokio::net::UdpSocket;

use super::{PORT_ZERO_RETRIES, bind_pair};
use crate::Reason;

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

const ANY_PORT: u16 = 0;
const TAKEN_BEFORE_SUCCESS: usize = 2;
const RESERVATION_ATTEMPTS: usize = 8;

fn address(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, port))
}

fn refusal(kind: io::ErrorKind) -> io::Error {
    io::Error::from(kind)
}

#[tokio::test]
async fn should_draw_a_fresh_port_when_the_udp_twin_is_taken() -> TestResult {
    let mut draws = 0;
    let (tcp, udp) = bind_pair(address(ANY_PORT), |shared| {
        draws += 1;
        let taken = draws <= TAKEN_BEFORE_SUCCESS;
        async move {
            if taken {
                Err(refusal(io::ErrorKind::AddrInUse))
            } else {
                UdpSocket::bind(shared).await
            }
        }
    })
    .await?;

    // The real bind after the staged refusals can collide too, and a redraw is correct then.
    assert!(
        draws > TAKEN_BEFORE_SUCCESS,
        "the staged refusals each cost a draw, got {draws}"
    );
    assert_eq!(
        tcp.local_addr()?,
        udp.local_addr()?,
        "both carriers share one port"
    );
    Ok(())
}

#[tokio::test]
async fn should_bind_a_free_fixed_port_for_both_carriers_in_one_draw() -> TestResult {
    for _ in 0..RESERVATION_ATTEMPTS {
        let probe = std::net::TcpListener::bind(address(ANY_PORT))?;
        let fixed = probe.local_addr()?.port();
        drop(probe);

        let mut draws = 0;
        let (tcp, udp) = match bind_pair(address(fixed), |shared| {
            draws += 1;
            UdpSocket::bind(shared)
        })
        .await
        {
            Ok(pair) => pair,
            // Another socket took the reserved number first: reserve a different one.
            Err(error) if error.io_kind() == Some(io::ErrorKind::AddrInUse) => continue,
            Err(error) => return Err(error.into()),
        };

        assert_eq!(draws, 1);
        assert_eq!(tcp.local_addr()?.port(), fixed);
        assert_eq!(udp.local_addr()?.port(), fixed);
        return Ok(());
    }
    Err("no free fixed port could be reserved".into())
}

#[tokio::test]
async fn should_give_up_after_the_retry_budget_when_every_udp_twin_is_taken() -> TestResult {
    let mut draws = 0;
    let Err(error) = bind_pair(address(ANY_PORT), |_| {
        draws += 1;
        async { Err::<UdpSocket, _>(refusal(io::ErrorKind::AddrInUse)) }
    })
    .await
    else {
        return Err("every draw collides, so the bind must fail".into());
    };

    assert_eq!(draws, PORT_ZERO_RETRIES + 1);
    assert_eq!(error.io_kind(), Some(io::ErrorKind::AddrInUse));
    assert_eq!(error.reason(), Reason::Network);
    Ok(())
}

#[tokio::test]
async fn should_bind_a_fixed_port_once_and_report_it_taken() -> TestResult {
    for _ in 0..RESERVATION_ATTEMPTS {
        let probe = std::net::TcpListener::bind(address(ANY_PORT))?;
        let fixed = probe.local_addr()?.port();
        drop(probe);

        let mut draws = 0;
        let Err(error) = bind_pair(address(fixed), |_| {
            draws += 1;
            async { Err::<UdpSocket, _>(refusal(io::ErrorKind::AddrInUse)) }
        })
        .await
        else {
            return Err("a taken fixed port must fail".into());
        };

        // Another socket took the reserved number before TCP bound it, so the UDP
        // boundary was never reached: reserve a different number and try again.
        if draws == 0 {
            continue;
        }
        assert_eq!(draws, 1);
        assert_eq!(error.io_kind(), Some(io::ErrorKind::AddrInUse));
        return Ok(());
    }
    Err("no free fixed port could be reserved".into())
}

#[tokio::test]
async fn should_not_redraw_for_an_error_other_than_address_in_use() -> TestResult {
    let mut draws = 0;
    let Err(error) = bind_pair(address(ANY_PORT), |_| {
        draws += 1;
        async { Err::<UdpSocket, _>(refusal(io::ErrorKind::PermissionDenied)) }
    })
    .await
    else {
        return Err("permission is not a collision, so the bind must fail".into());
    };

    assert_eq!(draws, 1);
    assert_eq!(error.io_kind(), Some(io::ErrorKind::PermissionDenied));
    Ok(())
}

#[cfg(feature = "test-util")]
#[tokio::test]
async fn a_bound_listener_renders_without_its_identity_or_sockets() -> TestResult {
    let fixture = crate::test_util::IdentityFixture::new(vec!["localhost".to_owned()])?;
    let listener = super::Listener::bind(address(ANY_PORT), fixture.identity()).await?;
    assert_eq!(format!("{listener:?}"), "Listener { .. }");
    assert_eq!(listener.local_addr()?.ip(), Ipv4Addr::LOCALHOST);
    Ok(())
}

/// Each carrier loads the identity again when it starts serving, so files gone
/// since the bind fail both: either failure stops its sibling, and the caller
/// gets one error while the second is logged rather than lost.
#[cfg(feature = "test-util")]
#[tokio::test]
async fn should_stop_both_carriers_and_log_the_second_when_the_identity_is_gone() -> TestResult {
    use std::convert::Infallible;

    use http::{Request, Response};
    use tokio_util::sync::CancellationToken;

    use crate::{Incoming, Outgoing};

    let capture = af_core::test_util::trace::Capture::install();
    let fixture = crate::test_util::IdentityFixture::new(vec!["localhost".to_owned()])?;
    let listener = super::Listener::bind(address(ANY_PORT), fixture.identity()).await?;
    drop(fixture);

    let served = listener
        .serve(
            |_info| {
                tower::service_fn(async |_request: Request<Incoming>| {
                    Ok::<_, Infallible>(Response::new(Outgoing::new(http_body_util::Empty::<
                        bytes::Bytes,
                    >::new())))
                })
            },
            CancellationToken::new(),
            CancellationToken::new(),
        )
        .await;

    assert!(served.is_err(), "no carrier can serve without its identity");
    let logged = capture.only("runner_stream_transport_ended");
    assert_eq!(
        logged.field("transport"),
        Some(crate::Transport::H3.as_str())
    );
    Ok(())
}
