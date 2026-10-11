//! Commands with no key to route by: one reply per primary, one from a
//! chosen node, or one from any node.
//!
//! Split from [`super`] for length. Every path here still runs under the one
//! deadline [`super`] applies, through the same `bounded` call site.

use redis::cluster_routing::{RoutingInfo, SingleNodeRoutingInfo};
use redis::{Cmd, FromRedisValue};

use super::{CMD_INFO, Dragonfly};
use crate::error::Result;

impl Dragonfly {
    /// One `INFO <section>` reply per PRIMARY, in the order the topology names
    /// them.
    ///
    /// `INFO` has no key, so a cluster connection fans it out and answers a
    /// reply per node — which is not the single bulk string a caller decoding
    /// `String` expects, and is why asking for one returns an unexpected-reply
    /// error rather than a number. A server statistic on a cluster is a SUM
    /// across primaries anyway: memory a fleet's keys occupy is spread over
    /// whichever shard owns each key, and a figure from one node is a fraction
    /// reported as a total.
    ///
    /// Routed per node the way [`Self::scan_keys`] walks them, and
    /// concurrently, because the nodes are independent.
    ///
    /// # Errors
    /// As [`Self::command`], and when the topology cannot be read.
    pub async fn info_per_primary(&self, section: &str) -> Result<Vec<String>> {
        let primaries = crate::topology::primaries(self).await?;
        let asks = primaries.into_iter().map(|node| {
            let mut cmd = redis::cmd(CMD_INFO);
            cmd.arg(section);
            async move {
                let routing = RoutingInfo::SingleNode(SingleNodeRoutingInfo::ByAddress {
                    host: node.host,
                    port: node.port,
                });
                self.route::<String>(CMD_INFO, section, &cmd, routing).await
            }
        });
        futures_util::future::try_join_all(asks).await
    }

    /// Runs one command on the node at `routing`, for the verbs that have no
    /// key to route by — a `SCAN` is a walk of one node.
    ///
    /// # Errors
    /// As [`Self::command`].
    pub(crate) async fn route<T: FromRedisValue>(
        &self,
        name: &'static str,
        context: &str,
        cmd: &Cmd,
        routing: RoutingInfo,
    ) -> Result<T> {
        let mut connection = self.connection.clone();
        let cmd = cmd.clone();
        self.bounded(name, context, async move {
            connection.route_command(cmd, routing).await
        })
        .await
    }

    /// One reply from ONE node, for a command that describes the DEPLOYMENT
    /// rather than a key.
    ///
    /// `CLUSTER INFO` and `COMMAND INFO` answer the same on every node — they
    /// say what the server IS and what it can do, not what it holds — so
    /// asking one node is asking all of them. Routed explicitly rather than
    /// left to the driver, which fans a keyless command out and answers a
    /// reply per node: not the single value a caller decoding one expects,
    /// which is the same trap [`Self::info_per_primary`] exists for.
    ///
    /// # Errors
    /// As [`Self::command`].
    pub(crate) async fn ask_one_node<T: FromRedisValue>(
        &self,
        name: &'static str,
        context: &str,
        cmd: &Cmd,
    ) -> Result<T> {
        self.route(
            name,
            context,
            cmd,
            RoutingInfo::SingleNode(SingleNodeRoutingInfo::Random),
        )
        .await
    }
}
