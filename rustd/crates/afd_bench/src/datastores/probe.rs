//! Connected-server identity and topology proof for a bench target.

use serde::{Deserialize, Serialize};
use sqlx::Row as _;

use super::{Datastores, command};
use crate::error::Result;
use crate::profile::Target;

/// The connected Postgres identity, with no configured URL or credential.
const POSTGRES_IDENTITY_QUERY: &str = "SELECT COALESCE(inet_server_addr()::text, 'local'), \
     COALESCE(inet_server_port(), 0), current_database(), version()";
const LOCAL_HOST: &str = "local";

/// Raw server and topology evidence collected from the connected datastores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatastoreProbe {
    /// Connected Postgres address, port, database and version.
    pub postgres_raw: String,
    /// `INFO server`, unmodified.
    pub redis_server_raw: String,
    /// `INFO replication`, unmodified.
    pub redis_replication_raw: String,
    /// `CLUSTER NODES`, or the exact refusal on a standalone server.
    pub redis_topology_raw: String,
    /// Every address advertised by Redis replication or cluster topology.
    pub discovered_hosts: Vec<String>,
}

impl Datastores {
    /// Refuse unsafe endpoints, connect, then verify server-reported topology.
    ///
    /// # Errors
    ///
    /// A datastore connection/probe failure, or [`crate::Error::UnsafeTarget`]
    /// when a local target advertises a remote node.
    pub async fn open_checked(
        target: &Target,
        database_url: &str,
        redis_url: &str,
        ca_cert: Option<String>,
    ) -> Result<Self> {
        let opened = Self::open(database_url, redis_url, ca_cert).await?;
        let _ = opened.probe(target).await?;
        Ok(opened)
    }

    /// Collect and validate connected-server identity and advertised topology.
    ///
    /// # Errors
    ///
    /// A datastore that will not answer, an unreadable Postgres identity, or a
    /// local target that reports any non-loopback host.
    pub async fn probe(&self, target: &Target) -> Result<DatastoreProbe> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(POSTGRES_IDENTITY_QUERY)
            .fetch_one(&mut *connection)
            .await?;
        let postgres_host: String = row.try_get(0)?;
        let postgres_port: i32 = row.try_get(1)?;
        let database: String = row.try_get(2)?;
        let postgres_version: String = row.try_get(3)?;
        let redis_server_raw = redis_info(self, command::SERVER).await?;
        let redis_replication_raw = redis_info(self, command::REPLICATION).await?;
        let mut cluster = redis::cmd(command::CLUSTER);
        cluster.arg(command::NODES);
        let (redis_topology_raw, cluster_hosts) = match self
            .queue
            .command::<String>(command::CLUSTER, command::NODES, &cluster)
            .await
        {
            Ok(raw) => {
                let hosts = cluster_hosts(&raw);
                (raw, hosts)
            }
            Err(refusal) => (format!("refused: {refusal}"), Vec::new()),
        };
        // `inet_server_addr()` is the server's own container-side interface,
        // not a client redirect or reconnect address. The configured Postgres
        // endpoint was already checked before this connection opened.
        let mut discovered_hosts = Vec::new();
        if let Some(master) = info_field(&redis_replication_raw, "master_host:") {
            discovered_hosts.push(master.to_owned());
        }
        discovered_hosts.extend(cluster_hosts);
        discovered_hosts.sort();
        discovered_hosts.dedup();
        for host in &discovered_hosts {
            if host != LOCAL_HOST {
                target.check_discovered_host("advertised datastore node", host)?;
            }
        }

        Ok(DatastoreProbe {
            postgres_raw: format!(
                "address={postgres_host}\nport={postgres_port}\ndatabase={database}\nversion={postgres_version}\n"
            ),
            redis_server_raw,
            redis_replication_raw,
            redis_topology_raw,
            discovered_hosts,
        })
    }
}

/// One raw `INFO` section.
async fn redis_info(stores: &Datastores, section: &'static str) -> Result<String> {
    let mut request = redis::cmd(command::INFO);
    request.arg(section);
    Ok(stores
        .queue
        .command(command::INFO, section, &request)
        .await?)
}

/// One `INFO` field, without changing its bytes beyond line endings.
fn info_field<'a>(raw: &'a str, prefix: &str) -> Option<&'a str> {
    raw.lines().find_map(|line| line.strip_prefix(prefix))
}

/// Hostnames and addresses advertised by `CLUSTER NODES`.
fn cluster_hosts(raw: &str) -> Vec<String> {
    raw.lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .filter_map(|address| address.split('@').next())
        .filter_map(|address| {
            if address.starts_with('[') {
                return address.find(']').map(|end| address[1..end].to_owned());
            }
            address
                .rsplit_once(':')
                .map(|(host, _port)| host.to_owned())
        })
        .collect()
}
