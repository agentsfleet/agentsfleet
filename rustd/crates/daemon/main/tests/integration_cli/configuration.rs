//! A spawned daemon applies its file before opening its HTTP listener.

use std::io::Write as _;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use super::{a_free_port, lane_knobs, spawn, stop_with, wait_until_ready};

#[test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
fn a_selected_file_drives_the_serving_daemon() {
    let port = a_free_port();
    let mut file = tempfile::NamedTempFile::new().expect("application file");
    writeln!(
        file,
        "[server]\nport = {port}\n[runner.lease]\nttl_seconds = 40\n\
         [runner.recovery]\nresume_from_checkpoint = false\n\
         offer_fallback_seconds = 2\nmax_concurrent_claims = 4"
    )
    .expect("deployment settings");
    let path = file.path().to_str().expect("UTF-8 fixture path");
    for through_environment in [false, true] {
        let mut knobs = lane_knobs();
        let args = if through_environment {
            knobs.push(("AGENTSFLEET_CONFIG_FILE", path.to_owned()));
            vec!["serve"]
        } else {
            vec!["--config", path, "serve"]
        };
        let mut child = spawn(&args, &knobs);
        wait_until_ready(port, &mut child);
        assert_eq!(stop_with("-TERM", child), 0);
    }
}

#[test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
fn an_explicit_port_overrides_the_file_before_typed_startup() {
    let port = a_free_port();
    let ignored = a_free_port();
    assert_ne!(port, ignored, "the two ports are distinct");
    let mut file = tempfile::NamedTempFile::new().expect("application file");
    writeln!(file, "[server]\nport = 'replaced-before-deserialization'")
        .expect("overridden file setting");
    let mut knobs = lane_knobs();
    knobs.push(("PORT", ignored.to_string()));
    let mut child = spawn(
        &[
            "--config",
            file.path().to_str().expect("UTF-8 fixture path"),
            "serve",
            "--port",
            &port.to_string(),
        ],
        &knobs,
    );
    wait_until_ready(port, &mut child);
    let overridden = SocketAddr::from((Ipv4Addr::LOCALHOST, ignored));
    TcpStream::connect_timeout(&overridden, Duration::from_millis(250))
        .expect_err("the environment port was overridden");
    assert_eq!(stop_with("-TERM", child), 0);
}
