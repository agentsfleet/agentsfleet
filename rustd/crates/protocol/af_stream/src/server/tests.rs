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

    assert_eq!(draws, TAKEN_BEFORE_SUCCESS + 1);
    assert_eq!(
        tcp.local_addr()?,
        udp.local_addr()?,
        "both carriers share one port"
    );
    Ok(())
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
