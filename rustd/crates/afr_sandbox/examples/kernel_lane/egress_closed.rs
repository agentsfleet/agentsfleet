//! What an allowlisted sandbox never reaches, whatever its set admits: a
//! resolver port on a listed address, the runner's own host, and, the other
//! way, a connection opened toward it from outside. Each closed path has a
//! control beside it that the same sandbox does reach, so a trial passes only
//! when the rule, and not a dead listener, is what stops the traffic.

use std::io::{self, Read as _, Write as _};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use afr_sandbox::egress_testing::{
    DNS_PORT, FAR_GREETING, FAR_HOST_SIDE, FAR_LISTED, FAR_PORT, Far, objects,
};
use afr_sandbox::{Engine, Limits, Network, SandboxRequest};
use libtest_mimic::Failed;

use crate::egress::{ERRNO, FAR_NAME, connect};
use crate::lane::Lane;
use crate::run::{expect, run, runtime, shell};

/// The port the runner's host answers on, on every address it holds.
const HOST_PORT: u16 = 8444;
/// The port a sandbox listens on for the inbound trial.
const SANDBOX_PORT: u16 = 8445;
/// What the host's listener and the sandbox's say to every connection.
const HOST_GREETING: &str = "host";
const SANDBOX_GREETING: &str = "inside";
/// What [`DATAGRAM`] prints when nothing answers.
const NO_ANSWER: &str = "timeout";
/// What the sandbox's listener prints after its greeting when nothing from
/// outside reached it.
const NONE_INBOUND: &str = "inbound 0";
/// How long after the listener starts the far host connects to it.
const INBOUND_AFTER: Duration = Duration::from_millis(1500);
/// The prefix of a scope's table name; the slot number follows it.
const TABLE_PREFIX: &str = "afegress";
/// Sends one datagram to `$1:$2` and prints the answer, `timeout`, or
/// `errno <n>`.
const DATAGRAM: &str = "python3 - \"$@\" <<'EOF'\nimport socket, sys\n\
     s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)\ns.settimeout(2)\ntry:\n    \
     s.sendto(b'q', (sys.argv[1], int(sys.argv[2])))\n    print(s.recv(16).decode())\n\
     except socket.timeout:\n    print('timeout')\nexcept OSError as e:\n    \
     print('errno', e.errno)\nEOF";

/// The command that sends one datagram to `host` on `port`.
fn datagram(host: Ipv4Addr, port: u16) -> String {
    format!("set -- {host} {port}; {DATAGRAM}")
}

/// Listens on [`SANDBOX_PORT`] in the foreground: proves itself alive by
/// reading its own greeting back over loopback, then for six seconds counts
/// every connection that reaches it, and prints both.
fn listen_for_inbound() -> String {
    format!(
        "python3 - <<'EOF'\nimport socket, time\n\
         s = socket.socket()\ns.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)\n\
         s.bind(('0.0.0.0', {SANDBOX_PORT}))\ns.listen()\ns.settimeout(0.2)\n\
         local = socket.create_connection(('127.0.0.1', {SANDBOX_PORT}), 2)\n\
         c, _ = s.accept()\nc.sendall(b'{SANDBOX_GREETING}')\nc.close()\n\
         print(local.recv(16).decode())\ninbound = 0\ndeadline = time.time() + 6\n\
         while time.time() < deadline:\n    try:\n        c, _ = s.accept()\n        \
         inbound += 1\n        c.close()\n    except socket.timeout:\n        pass\n\
         print('inbound', inbound)\nEOF"
    )
}

/// The sandbox side of the scope `before` did not list: `10.69.<slot>.2`,
/// read from the new table's slot.
fn sandbox_address(before: &[String]) -> Result<Ipv4Addr, Failed> {
    let slot = objects()?
        .into_iter()
        .filter(|name| !before.contains(name))
        .find_map(|name| name.strip_prefix(TABLE_PREFIX)?.parse::<u8>().ok())
        .ok_or("the live scope's table names its slot")?;
    Ok(Ipv4Addr::new(10, 69, slot, 2))
}

/// Answers every connection on `port`, on every address of the host's
/// namespace, with the host's greeting.
fn answer_on_host(port: u16) -> Result<(), Failed> {
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port))?;
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let _said = stream.write_all(HOST_GREETING.as_bytes());
        }
    });
    Ok(())
}

/// What a connection from the host itself to `address` reads.
fn read_from(address: SocketAddr) -> Result<String, Failed> {
    let mut said = String::new();
    TcpStream::connect(address)?.read_to_string(&mut said)?;
    Ok(said)
}

/// Port 53 on a listed address, the same address on its own port as the
/// control, and the runner's host.
fn closed_paths() -> [String; 3] {
    [
        datagram(FAR_LISTED, DNS_PORT),
        datagram(FAR_LISTED, FAR_PORT),
        connect(&FAR_HOST_SIDE.to_string(), HOST_PORT),
    ]
}

/// Runs `listen` while the far host, [`INBOUND_AFTER`] in, connects to `own`,
/// and returns what each got.
fn while_far_knocks<T>(
    far: &Far,
    own: SocketAddr,
    listen: impl FnOnce() -> T,
) -> (T, io::Result<String>) {
    thread::scope(|threads| {
        let knock = threads.spawn(|| {
            thread::sleep(INBOUND_AFTER);
            far.connect_from(own)
        });
        let listened = listen();
        let knocked = knock.join().unwrap_or_else(|_panicked| {
            Err(io::Error::other("the far host's connection panicked"))
        });
        (listened, knocked)
    })
}

/// An allowlisted sandbox gets no answer from the resolver port of a listed
/// address, though the same address answers on another port; cannot reach
/// the runner's host, which the lane itself reaches; and accepts no
/// connection from outside, though its own listener answers locally.
pub(crate) fn allow_list_closes_dns_the_host_and_inbound(lane: &Lane) -> Result<(), Failed> {
    let far = Far::start()?;
    let allowlist = Far::allowlist(FAR_NAME)?;
    answer_on_host(HOST_PORT)?;
    let host_side = SocketAddr::from((FAR_HOST_SIDE, HOST_PORT));
    let host_answers = read_from(host_side)?;
    let scripts = closed_paths();
    let before = objects()?;
    let runtime = runtime();
    let engine = lane.engine();
    let request = SandboxRequest::new("egress-closed", Limits::default())
        .with_network(Network::Allowed(&allowlist));
    let sandbox = runtime.block_on(engine.prepare(request))?;
    let own = match sandbox_address(&before) {
        Ok(address) => SocketAddr::from((address, SANDBOX_PORT)),
        Err(failed) => {
            runtime.block_on(sandbox.destroy())?;
            return Err(failed);
        }
    };
    let (listened, inbound) = while_far_knocks(&far, own, || {
        runtime.block_on(run(sandbox.executor(), shell(&listen_for_inbound())))
    });
    let said = runtime.block_on(async {
        let mut said = Vec::with_capacity(scripts.len());
        for script in &scripts {
            let outcome = run(sandbox.executor(), shell(script)).await;
            said.push(outcome.map(|outcome| outcome.output.trim().to_owned()));
        }
        sandbox.destroy().await?;
        said.into_iter().collect::<Result<Vec<_>, Failed>>()
    })?;
    let listened = listened?.output.trim().to_owned();

    expect(
        host_answers == HOST_GREETING,
        format!("the host's listener answers the host: {host_answers:?}"),
    )?;
    expect(
        inbound.is_err() && listened == format!("{SANDBOX_GREETING}\n{NONE_INBOUND}"),
        format!("a listening sandbox takes no connection from outside: {inbound:?}, {listened:?}"),
    )?;
    expect(
        said.first().is_some_and(|answer| answer == NO_ANSWER)
            && said.get(1).is_some_and(|answer| answer == FAR_GREETING)
            && said.get(2).is_some_and(|answer| answer.starts_with(ERRNO)),
        format!("port 53 closed, the address open, the host closed: {said:?}"),
    )
}
